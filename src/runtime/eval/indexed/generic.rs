use super::semantic::{SemanticPools, TypeTag};
use super::{IrFunctionId, IrVerifyError, TypeId as GroundTypeId};
use crate::symbol::Name;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);

/// A proof belongs to one immutable program. Serials are never reused after rewind.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct OwnerProof {
    root: u64,
    serial: u64,
}

macro_rules! evidence_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub(in crate::runtime::eval) struct $name {
            index: u32,
            proof: OwnerProof,
        }
    };
}

evidence_id!(SchemeScopeId);
evidence_id!(PhysicalLayoutId);
evidence_id!(InstantiationId);
evidence_id!(ForwardingId);
evidence_id!(TypeTemplateId);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum TypeRef {
    Ground(GroundTypeId),
    Rigid(u32),
    Template(TypeTemplateId),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum QuantifierKind { Type, Row }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum ParameterMode { PositionalOrNamed, NamedOnly }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum CallableKind { Pure, Proc }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct TemplateParameter { pub label: Name, pub ty: TypeRef, pub mode: ParameterMode, pub defaulted: bool, pub rest: bool }

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum TypeTemplate {
    Optional(TypeRef),
    List(TypeRef),
    Stream(TypeRef),
    Map { key: TypeRef, value: TypeRef },
    Result { ok: TypeRef, error: TypeRef },
    Record { fields: Box<[(Name, TypeRef)]>, row_tail: Option<u32> },
    Arrow { kind: CallableKind, parameters: Box<[TemplateParameter]>, result: TypeRef, effects: Box<[crate::syntax::node::Effect]> },
}

/// The declaration fixes wrapping before any concrete payload is supplied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum GenericReturnPlan {
    Value,
    Result,
    Unit,
    ResultUnit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum Requirement {
    Projection { receiver: TypeRef, receiver_parameter: u32, field: Name, result: TypeRef },
    Add { left: TypeRef, right: TypeRef, result: TypeRef },
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct SchemeScope {
    pub owner: IrFunctionId,
    pub quantifiers: Box<[QuantifierKind]>,
    pub parameters: Box<[TypeRef]>,
    pub parameter_names: Box<[Name]>,
    pub parameter_flags: Box<[u8]>,
    pub kind: CallableKind,
    pub result: TypeRef,
    pub requirements: Box<[Requirement]>,
    pub return_plan: GenericReturnPlan,
}

/// Field order describes the actual constructor storage, independently of semantic shape order.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PhysicalLayout {
    pub record_type: GroundTypeId,
    pub fields: Box<[(Name, GroundTypeId)]>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum ConcreteOperationId {
    AddInt,
    AddFloat,
    AddStr,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) enum RequirementWitness {
    Projection { layout: PhysicalLayoutId, field_slot: u32, result: GroundTypeId },
    Add { operation: ConcreteOperationId, left: GroundTypeId, right: GroundTypeId, result: GroundTypeId },
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(in crate::runtime::eval) struct Instantiation {
    pub scope: SchemeScopeId,
    pub substitutions: Box<[GroundTypeId]>,
    pub parameter_types: Box<[GroundTypeId]>,
    pub result_type: GroundTypeId,
    pub requirements: Box<[RequirementWitness]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum ForwardedRequirement {
    Caller(u32),
    Fixed(RequirementWitness),
}

/// All concrete forwarding edges are prepared once; execution selects a stored edge.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct ForwardingPlan {
    pub caller: SchemeScopeId,
    pub callee: SchemeScopeId,
    pub substitutions: Box<[TypeRef]>,
    pub requirements: Box<[ForwardedRequirement]>,
    pub instances: Box<[(InstantiationId, InstantiationId)]>,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) enum CallEvidence {
    Ground(InstantiationId),
    Forwarded(ForwardingId),
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) enum InstructionOwner {
    Function(IrFunctionId),
    Driver(u32),
}

impl PartialEq for InstructionOwner {
    fn eq(&self, other: &Self) -> bool {
        match (*self, *other) { (Self::Function(a), Self::Function(b)) => a == b, (Self::Driver(a), Self::Driver(b)) => a == b, _ => false }
    }
}
impl Eq for InstructionOwner {}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct SolvedCall {
    pub instruction: u32,
    pub caller: InstructionOwner,
    pub target: SchemeScopeId,
    pub evidence: CallEvidence,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct SolvedArgument {
    pub call_instruction: u32,
    pub parameter: u32,
    pub source_instruction: Option<u32>,
    pub ty: TypeRef,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct SolvedRequirementUse {
    pub instruction: u32,
    pub scope: SchemeScopeId,
    pub requirement: u32,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::runtime::eval) struct SolvedRecordLayout {
    pub instruction: u32,
    pub owner: InstructionOwner,
    pub layout: PhysicalLayoutId,
}

#[derive(Clone, Debug)]
struct Entry<T> {
    serial: u64,
    value: T,
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct GenericEvidenceStore {
    root: u64,
    scopes: Vec<Entry<SchemeScope>>,
    layouts: Vec<Entry<PhysicalLayout>>,
    instances: Vec<Entry<Instantiation>>,
    forwarding: Vec<Entry<ForwardingPlan>>,
    templates: Vec<Entry<TypeTemplate>>,
    calls: Vec<SolvedCall>,
    arguments: Vec<SolvedArgument>,
    uses: Vec<SolvedRequirementUse>,
    constructors: Vec<SolvedRecordLayout>,
    function_scopes: Vec<(IrFunctionId, SchemeScopeId)>,
}

fn failure(message: &'static str) -> IrVerifyError { IrVerifyError::new(message) }

fn owned<'a, T>(root: u64, entries: &'a [Entry<T>], index: u32, proof: OwnerProof) -> Result<&'a T, IrVerifyError> {
    if proof.root != root { return Err(failure("generic evidence belongs to a foreign program")); }
    let entry = entries.get(index as usize).ok_or_else(|| failure("generic evidence id is out of bounds"))?;
    if entry.serial != proof.serial { return Err(failure("generic evidence was retired by rewind")); }
    Ok(&entry.value)
}

impl GenericEvidenceStore {
    pub fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>() + self.scopes.capacity() * size_of::<Entry<SchemeScope>>()
            + self.layouts.capacity() * size_of::<Entry<PhysicalLayout>>()
            + self.instances.capacity() * size_of::<Entry<Instantiation>>()
            + self.forwarding.capacity() * size_of::<Entry<ForwardingPlan>>()
            + self.templates.capacity() * size_of::<Entry<TypeTemplate>>()
            + self.arguments.capacity() * size_of::<SolvedArgument>() + self.calls.capacity() * size_of::<SolvedCall>() + self.uses.capacity() * size_of::<SolvedRequirementUse>()
            + self.constructors.capacity() * size_of::<SolvedRecordLayout>() + self.function_scopes.capacity() * size_of::<(IrFunctionId, SchemeScopeId)>()
            + self.scopes.iter().map(|entry| entry.value.quantifiers.len() * size_of::<QuantifierKind>() + entry.value.parameters.len() * size_of::<TypeRef>() + entry.value.parameter_names.len() * size_of::<Name>() + entry.value.parameter_flags.len() * size_of::<u8>() + entry.value.requirements.len() * size_of::<Requirement>()).sum::<usize>()
            + self.layouts.iter().map(|entry| entry.value.fields.len() * size_of::<(Name, GroundTypeId)>()).sum::<usize>()
            + self.instances.iter().map(|entry| (entry.value.substitutions.len() + entry.value.parameter_types.len()) * size_of::<GroundTypeId>() + entry.value.requirements.len() * size_of::<RequirementWitness>()).sum::<usize>()
            + self.forwarding.iter().map(|entry| entry.value.substitutions.len() * size_of::<TypeRef>() + entry.value.requirements.len() * size_of::<ForwardedRequirement>() + entry.value.instances.len() * size_of::<(InstantiationId, InstantiationId)>()).sum::<usize>()
            + self.templates.iter().map(|entry| match &entry.value { TypeTemplate::Record { fields, .. } => fields.len() * size_of::<(Name, TypeRef)>(), TypeTemplate::Arrow { parameters, effects, .. } => parameters.len() * size_of::<TemplateParameter>() + effects.len() * size_of::<crate::syntax::node::Effect>(), _ => 0 }).sum::<usize>()
    }
    pub fn shrink_to_fit(&mut self) { self.scopes.shrink_to_fit(); self.layouts.shrink_to_fit(); self.instances.shrink_to_fit(); self.forwarding.shrink_to_fit(); self.templates.shrink_to_fit(); self.calls.shrink_to_fit(); self.arguments.shrink_to_fit(); self.uses.shrink_to_fit(); self.constructors.shrink_to_fit(); self.function_scopes.shrink_to_fit(); }
    pub fn instances(&self) -> impl Iterator<Item = (InstantiationId, &Instantiation)> {
        self.instances.iter().enumerate().map(|(index, entry)| (InstantiationId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_scope_mut(&mut self, id: SchemeScopeId) -> Result<&mut SchemeScope, IrVerifyError> {
        self.scope(id)?;
        Ok(&mut self.scopes[id.index as usize].value)
    }
    #[cfg(test)]
    pub(in crate::runtime::eval) fn test_forwarding_mut(&mut self, id: ForwardingId) -> Result<&mut ForwardingPlan, IrVerifyError> {
        self.forwarding(id)?;
        Ok(&mut self.forwarding[id.index as usize].value)
    }
    pub fn scopes(&self) -> impl Iterator<Item = (SchemeScopeId, &SchemeScope)> {
        self.scopes.iter().enumerate().map(|(index, entry)| (SchemeScopeId { index: index as u32, proof: OwnerProof { root: self.root, serial: entry.serial } }, &entry.value))
    }
    pub fn scope_for_function(&self, owner: IrFunctionId) -> Option<SchemeScopeId> { self.function_scopes.binary_search_by_key(&owner.raw(), |entry| entry.0.raw()).ok().map(|index| self.function_scopes[index].1) }
    pub fn calls(&self) -> &[SolvedCall] { &self.calls }
    pub fn arguments(&self) -> &[SolvedArgument] { &self.arguments }
    pub fn call_arguments(&self, instruction: u32) -> &[SolvedArgument] {
        let start = self.arguments.partition_point(|argument| argument.call_instruction < instruction);
        let end = self.arguments.partition_point(|argument| argument.call_instruction <= instruction);
        &self.arguments[start..end]
    }
    pub fn requirement_uses(&self) -> &[SolvedRequirementUse] { &self.uses }
    pub fn constructors(&self) -> &[SolvedRecordLayout] { &self.constructors }
    pub fn scope(&self, id: SchemeScopeId) -> Result<&SchemeScope, IrVerifyError> { owned(self.root, &self.scopes, id.index, id.proof) }
    pub fn layout(&self, id: PhysicalLayoutId) -> Result<&PhysicalLayout, IrVerifyError> { owned(self.root, &self.layouts, id.index, id.proof) }
    pub fn instance(&self, id: InstantiationId) -> Result<&Instantiation, IrVerifyError> { owned(self.root, &self.instances, id.index, id.proof) }
    pub fn forwarding(&self, id: ForwardingId) -> Result<&ForwardingPlan, IrVerifyError> { owned(self.root, &self.forwarding, id.index, id.proof) }
    pub fn template(&self, id: TypeTemplateId) -> Result<&TypeTemplate, IrVerifyError> { owned(self.root, &self.templates, id.index, id.proof) }
    pub fn call(&self, instruction: u32) -> Option<&SolvedCall> { self.calls.binary_search_by_key(&instruction, |call| call.instruction).ok().map(|i| &self.calls[i]) }
    pub fn requirement_use(&self, instruction: u32) -> Option<&SolvedRequirementUse> { self.uses.binary_search_by_key(&instruction, |use_| use_.instruction).ok().map(|i| &self.uses[i]) }
    pub fn constructor(&self, instruction: u32) -> Option<&SolvedRecordLayout> { self.constructors.binary_search_by_key(&instruction, |record| record.instruction).ok().map(|i| &self.constructors[i]) }

    pub fn forwarded_instance(&self, plan: ForwardingId, caller: InstantiationId) -> Result<InstantiationId, IrVerifyError> {
        self.instance(caller)?;
        { let edges = &self.forwarding(plan)?.instances; edges.binary_search_by_key(&caller.index, |edge| edge.0.index).ok().filter(|&index| edges[index].0 == caller).map(|index| edges[index].1) }
            .ok_or_else(|| failure("generic forwarding edge was not prepared"))
    }

    fn verify_type(pools: &SemanticPools, ty: GroundTypeId) -> Result<(), IrVerifyError> {
        let concrete = pools.to_type(ty)?;
        if contains_unresolved(&concrete) { return Err(failure("unresolved types cannot certify generic evidence")); }
        Ok(())
    }

    fn verify_reference(&self, pools: &SemanticPools, scope: &SchemeScope, reference: TypeRef) -> Result<(), IrVerifyError> {
        self.verify_reference_inner(pools, scope, reference, &mut Vec::new())
    }

    fn verify_reference_inner(&self, pools: &SemanticPools, scope: &SchemeScope, reference: TypeRef, active: &mut Vec<TypeTemplateId>) -> Result<(), IrVerifyError> {
        if active.len() > 256 { return Err(failure("generic type template exceeds depth limit")); }
        match reference {
            TypeRef::Ground(ty) => Self::verify_type(pools, ty),
            TypeRef::Rigid(index) if scope.quantifiers.get(index as usize) == Some(&QuantifierKind::Type) => Ok(()),
            TypeRef::Rigid(_) => Err(failure("rigid generic variable is out of scope")),
            TypeRef::Template(id) => {
                if active.contains(&id) { return Err(failure("generic type template contains a cycle")); }
                active.push(id);
                let template = self.template(id)?;
                let references = match template {
                    TypeTemplate::Optional(inner) | TypeTemplate::List(inner) | TypeTemplate::Stream(inner) => vec![*inner],
                    TypeTemplate::Map { key, value } => vec![*key, *value],
                    TypeTemplate::Result { ok, error } => vec![*ok, *error],
                    TypeTemplate::Record { fields, row_tail } => {
                        if let Some(index) = row_tail && scope.quantifiers.get(*index as usize) != Some(&QuantifierKind::Row) { return Err(failure("record row tail is not a scoped row quantifier")); }
                        let names = fields.iter().map(|field| field.0).collect::<std::collections::BTreeSet<_>>();
                        if names.len() != fields.len() { return Err(failure("record template has duplicate fields")); }
                        fields.iter().map(|field| field.1).collect()
                    }
                    TypeTemplate::Arrow { parameters, result, .. } => parameters.iter().map(|parameter| parameter.ty).chain([*result]).collect(),
                };
                for reference in references { self.verify_reference_inner(pools, scope, reference, active)?; }
                active.pop();
                Ok(())
            }
        }
    }

    fn verify_witness(&self, pools: &SemanticPools, requirement: &Requirement, substitutions: &[GroundTypeId], parameter_types: &[GroundTypeId], witness: RequirementWitness) -> Result<(), IrVerifyError> {
        let mut memo = rustc_hash::FxHashMap::default();
        let mut expected = |reference| self.expand(pools, reference, substitutions, &mut memo);
        match (requirement, witness) {
            (Requirement::Projection { receiver, receiver_parameter, field, result }, RequirementWitness::Projection { layout, field_slot, result: actual }) => {
                let layout = self.layout(layout)?;
                if parameter_types.get(*receiver_parameter as usize) != Some(&layout.record_type) { return Err(failure("projection layout belongs to a different actual argument")); }
                let selected = layout.fields.get(field_slot as usize).ok_or_else(|| failure("projection field slot is out of bounds"))?;
                if !self.parameter_type_accepts(*receiver, &expected(*receiver)?, &pools.to_type(layout.record_type)?)? || selected.0 != *field || selected.1 != actual || pools.to_type(actual)? != expected(*result)? {
                    return Err(failure("projection evidence disagrees with its scoped requirement"));
                }
            }
            (Requirement::Add { left, right, result }, RequirementWitness::Add { operation, left: actual_left, right: actual_right, result: actual_result }) => {
                if [expected(*left)?, expected(*right)?, expected(*result)?] != [pools.to_type(actual_left)?, pools.to_type(actual_right)?, pools.to_type(actual_result)?] {
                    return Err(failure("operation evidence disagrees with its scoped requirement"));
                }
                let left = pools.type_tag(actual_left)?;
                let right = pools.type_tag(actual_right)?;
                let result = pools.type_tag(actual_result)?;
                let valid = match operation {
                    ConcreteOperationId::AddInt => matches!(left, TypeTag::Int | TypeTag::UInt) && matches!(right, TypeTag::Int | TypeTag::UInt) && result == TypeTag::Int,
                    ConcreteOperationId::AddFloat => left == TypeTag::Float && right == TypeTag::Float && result == TypeTag::Float,
                    ConcreteOperationId::AddStr => left == TypeTag::Str && right == TypeTag::Str && result == TypeTag::Str,
                };
                if !valid { return Err(failure("concrete operation has unsupported operand or result types")); }
            }
            _ => return Err(failure("generic witness has the wrong requirement kind")),
        }
        Ok(())
    }

    pub(super) fn expand(&self, pools: &SemanticPools, reference: TypeRef, substitutions: &[GroundTypeId], memo: &mut rustc_hash::FxHashMap<TypeRef, crate::sema::types::Type>) -> Result<crate::sema::types::Type, IrVerifyError> {
        use crate::sema::types::Type;
        if let Some(ty) = memo.get(&reference) { return Ok(ty.clone()); }
        let ty = match reference {
            TypeRef::Ground(ty) => pools.to_type(ty)?,
            TypeRef::Rigid(index) => pools.to_type(*substitutions.get(index as usize).ok_or_else(|| failure("template substitution is out of scope"))?)?,
            TypeRef::Template(id) => match self.template(id)? {
                TypeTemplate::Optional(inner) => Type::Optional(Box::new(self.expand(pools, *inner, substitutions, memo)?)),
                TypeTemplate::List(inner) => Type::List(Box::new(self.expand(pools, *inner, substitutions, memo)?)),
                TypeTemplate::Stream(inner) => Type::Stream(Box::new(self.expand(pools, *inner, substitutions, memo)?)),
                TypeTemplate::Map { key, value } => Type::Map(Box::new(self.expand(pools, *key, substitutions, memo)?), Box::new(self.expand(pools, *value, substitutions, memo)?)),
                TypeTemplate::Result { ok, error } => Type::Result(Box::new(self.expand(pools, *ok, substitutions, memo)?), Box::new(self.expand(pools, *error, substitutions, memo)?)),
                TypeTemplate::Record { fields, row_tail } => {
                    let mut row = std::collections::BTreeMap::new();
                    for &(name, field) in fields { row.insert(name, self.expand(pools, field, substitutions, memo)?); }
                    if let Some(index) = row_tail {
                        let Type::Record(tail) = pools.to_type(*substitutions.get(*index as usize).ok_or_else(|| failure("row substitution is out of scope"))?)? else { return Err(failure("row substitution is not a closed known record")); };
                        for (name, ty) in tail {
                            if row.insert(name, ty).is_some() { return Err(failure("row substitution violates an explicit field lacks constraint")); }
                        }
                    }
                    Type::Record(row)
                }
                TypeTemplate::Arrow { .. } => return Err(failure("signature-bearing callable ground evidence is not prepared")),
            },
        };
        memo.insert(reference, ty.clone());
        Ok(ty)
    }

    fn parameter_type_accepts(&self, reference: TypeRef, formal: &crate::sema::types::Type, actual: &crate::sema::types::Type) -> Result<bool, IrVerifyError> {
        if let TypeRef::Template(id) = reference && matches!(self.template(id)?, TypeTemplate::Record { row_tail: Some(_), .. }) { return Ok(formal == actual); }
        Ok(parameter_accepts(formal, actual))
    }

    pub(super) fn verify(&self, pools: &SemanticPools, function_count: usize, instruction_owners: &[Option<InstructionOwner>]) -> Result<(), IrVerifyError> {
        pools.verify()?;
        let mut expected_index = self.scopes().map(|(id, scope)| (scope.owner, id)).collect::<Vec<_>>();
        expected_index.sort_unstable_by_key(|entry| entry.0.raw());
        if self.function_scopes != expected_index { return Err(failure("generic function scope index is foreign or stale")); }
        let mut owners = std::collections::BTreeSet::new();
        for entry in &self.scopes {
            let scope = &entry.value;
            if scope.owner.index() >= function_count { return Err(failure("generic scope function is out of bounds")); }
            if scope.parameters.len() != scope.parameter_names.len() || scope.parameters.len() != scope.parameter_flags.len() || scope.parameter_flags.iter().any(|flags| flags & !0b111 != 0) { return Err(failure("generic parameter metadata is inconsistent")); }
            if !owners.insert(scope.owner) { return Err(failure("generic function has multiple scheme scopes")); }
            for reference in scope.parameters.iter().copied().chain([scope.result]) { self.verify_reference(pools, scope, reference)?; }
            self.verify_return_plan(pools, scope)?;
            for requirement in &scope.requirements {
                match requirement {
                    Requirement::Projection { receiver, receiver_parameter, result, .. } => {
                        if scope.parameters.get(*receiver_parameter as usize) != Some(receiver) { return Err(failure("projection receiver does not denote its scoped parameter")); }
                        for reference in [receiver, result] { self.verify_reference(pools, scope, *reference)?; }
                    }
                    Requirement::Add { left, right, result } => for reference in [left, right, result] { self.verify_reference(pools, scope, *reference)?; },
                }
            }
        }
        for entry in &self.layouts {
            let layout = &entry.value;
            let (names, types) = pools.record_fields(layout.record_type)?;
            if names.len() != layout.fields.len() { return Err(failure("physical record layout width disagrees with semantic shape")); }
            let mut seen = std::collections::BTreeSet::new();
            for &(name, ty) in &layout.fields {
                let index = names.iter().position(|field| *field == name).ok_or_else(|| failure("physical layout field is absent from semantic shape"))?;
                if !seen.insert(name) || Some(ty) != GroundTypeId::from_raw(types[index]) { return Err(failure("physical layout field type or identity is inconsistent")); }
                Self::verify_type(pools, ty)?;
            }
        }
        for entry in &self.instances {
            let instance = &entry.value;
            let scope = self.scope(instance.scope)?;
            if instance.substitutions.len() != scope.quantifiers.len() || instance.requirements.len() != scope.requirements.len() || instance.parameter_types.len() != scope.parameters.len() { return Err(failure("generic instantiation arity disagrees with its scope")); }
            for (&ty, kind) in instance.substitutions.iter().zip(&scope.quantifiers) {
                Self::verify_type(pools, ty)?;
                if *kind == QuantifierKind::Row && pools.type_tag(ty)? != TypeTag::Record { return Err(failure("row substitution is not a closed known record")); }
            }
            let mut memo = rustc_hash::FxHashMap::default();
            for (&formal, &actual) in scope.parameters.iter().zip(&instance.parameter_types) {
                Self::verify_type(pools, actual)?;
                if !self.parameter_type_accepts(formal, &self.expand(pools, formal, &instance.substitutions, &mut memo)?, &pools.to_type(actual)?)? { return Err(failure("generic argument ground type disagrees with instantiated template")); }
            }
            Self::verify_type(pools, instance.result_type)?;
            if self.expand(pools, scope.result, &instance.substitutions, &mut memo)? != pools.to_type(instance.result_type)? { return Err(failure("generic result ground type disagrees with instantiated template")); }
            for (requirement, &witness) in scope.requirements.iter().zip(&instance.requirements) { self.verify_witness(pools, requirement, &instance.substitutions, &instance.parameter_types, witness)?; }
        }
        for entry in &self.forwarding {
            let plan = &entry.value;
            let caller = self.scope(plan.caller)?;
            let callee = self.scope(plan.callee)?;
            if plan.substitutions.len() != callee.quantifiers.len() || plan.requirements.len() != callee.requirements.len() { return Err(failure("generic forwarding arity disagrees with callee")); }
            for (&reference, kind) in plan.substitutions.iter().zip(&callee.quantifiers) {
                if let TypeRef::Rigid(index) = reference {
                    if caller.quantifiers.get(index as usize) != Some(kind) { return Err(failure("forwarding changes quantifier kind or scope")); }
                } else { self.verify_reference(pools, caller, reference)?; }
            }
            for requirement in &plan.requirements {
                if let ForwardedRequirement::Caller(index) = requirement && *index as usize >= caller.requirements.len() { return Err(failure("forwarded requirement is out of caller scope")); }
            }
            self.verify_symbolic_forwarding(pools, plan)?;
            let mut previous = None;
            for &(from, to) in &plan.instances {
                if previous.is_some_and(|index| index >= from.index) { return Err(failure("generic forwarding contains ambiguous or unsorted edges")); }
                previous = Some(from.index);
                let from = self.instance(from)?;
                let to = self.instance(to)?;
                if from.scope != plan.caller || to.scope != plan.callee { return Err(failure("generic forwarding instance belongs to the wrong scope")); }
                let mut memo = rustc_hash::FxHashMap::default();
                let substitutions = plan.substitutions.iter().map(|reference| self.expand(pools, *reference, &from.substitutions, &mut memo)).collect::<Result<Vec<_>, _>>()?;
                let witnesses = plan.requirements.iter().map(|requirement| match requirement { ForwardedRequirement::Caller(i) => from.requirements[*i as usize], ForwardedRequirement::Fixed(witness) => *witness }).collect::<Vec<_>>();
                let actual = to.substitutions.iter().map(|&ty| pools.to_type(ty)).collect::<Result<Vec<_>, _>>()?;
                if substitutions != actual || witnesses.as_slice() != to.requirements.as_ref() { return Err(failure("precomputed forwarding edge disagrees with canonical map")); }
            }
        }
        let check_owner = |instruction: u32, owner| if instruction_owners.get(instruction as usize) == Some(&Some(owner)) { Ok(()) } else { Err(failure("solved generic instruction has a foreign or missing owner")) };
        verify_instruction_order(self.calls.iter().map(|call| call.instruction))?;
        verify_instruction_order(self.uses.iter().map(|use_| use_.instruction))?;
        verify_instruction_order(self.constructors.iter().map(|record| record.instruction))?;
        for call in &self.calls {
            check_owner(call.instruction, call.caller)?;
            self.scope(call.target)?;
            match call.evidence {
                CallEvidence::Ground(instance) if self.instance(instance)?.scope == call.target => {},
                CallEvidence::Forwarded(plan) if self.forwarding(plan)?.callee == call.target && InstructionOwner::Function(self.scope(self.forwarding(plan)?.caller)?.owner) == call.caller => {},
                _ => return Err(failure("solved generic call evidence targets the wrong scope")),
            }
        }
        let mut previous_argument = None;
        for argument in &self.arguments {
            let key = (argument.call_instruction, argument.parameter);
            if previous_argument.is_some_and(|previous| previous >= key) { return Err(failure("generic arguments are not unique and sorted")); }
            previous_argument = Some(key);
            let call = self.call(argument.call_instruction).ok_or_else(|| failure("generic argument has no solved call"))?;
            if let Some(source) = argument.source_instruction { check_owner(source, call.caller)?; }
            if argument.parameter as usize >= self.scope(call.target)?.parameters.len() { return Err(failure("generic argument parameter is out of bounds")); }
            match call.evidence {
                CallEvidence::Ground(instance) => {
                    let TypeRef::Ground(ty) = argument.ty else { return Err(failure("ground generic call argument has an unbound template")); };
                    Self::verify_type(pools, ty)?;
                    if self.instance(instance)?.parameter_types[argument.parameter as usize] != ty { return Err(failure("ground argument disagrees with its call instance")); }
                }
                CallEvidence::Forwarded(id) => {
                    let forwarding = self.forwarding(id)?;
                    self.verify_reference(pools, self.scope(forwarding.caller)?, argument.ty)?;
                    for &(from, to) in &forwarding.instances {
                        let ty = self.expand(pools, argument.ty, &self.instance(from)?.substitutions, &mut rustc_hash::FxHashMap::default())?;
                        if ty != pools.to_type(self.instance(to)?.parameter_types[argument.parameter as usize])? { return Err(failure("forwarded argument disagrees with its call instance")); }
                    }
                }
            }
        }
        for call in &self.calls {
            let arguments = self.call_arguments(call.instruction);
            if arguments.len() != self.scope(call.target)?.parameters.len() || arguments.iter().enumerate().any(|(index, argument)| argument.parameter as usize != index) { return Err(failure("generic call lacks complete argument type evidence")); }
        }
        for use_ in &self.uses {
            let scope = self.scope(use_.scope)?;
            check_owner(use_.instruction, InstructionOwner::Function(scope.owner))?;
            if use_.requirement as usize >= scope.requirements.len() { return Err(failure("generic instruction requirement is out of scope")); }
        }
        for record in &self.constructors { check_owner(record.instruction, record.owner)?; self.layout(record.layout)?; }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::semantic::SemanticPoolBuilder;
    use crate::sema::types::Type;
    use std::collections::BTreeMap;

    fn scalar(pools: &mut SemanticPools, builder: &mut SemanticPoolBuilder, ty: Type) -> GroundTypeId {
        builder.intern_type(pools, &ty).unwrap()
    }

    fn identity(owner: usize) -> SchemeScope {
        SchemeScope { owner: IrFunctionId::new(owner).unwrap(), quantifiers: Box::new([QuantifierKind::Type]), parameters: Box::new([TypeRef::Rigid(0)]), parameter_names: Box::new([Name::intern("value")]), parameter_flags: Box::new([0]), kind: CallableKind::Pure, result: TypeRef::Rigid(0), requirements: Box::new([]), return_plan: GenericReturnPlan::Value }
    }

    fn addition(owner: usize) -> SchemeScope {
        let mut scope = identity(owner);
        scope.parameters = Box::new([TypeRef::Rigid(0), TypeRef::Rigid(0)]);
        scope.parameter_names = Box::new([Name::intern("left"), Name::intern("right")]);
        scope.parameter_flags = Box::new([0, 0]);
        scope.requirements = Box::new([Requirement::Add { left: TypeRef::Rigid(0), right: TypeRef::Rigid(0), result: TypeRef::Rigid(0) }]);
        scope
    }

    #[test]
    fn prepared_component_templates_remap_sparse_member_binders_and_reject_siblings() {
        use crate::sema::inference::{Arrow, Atom, CallableKind, ComponentMember, EffectSet, EffectSummary, Generalization, InferenceContext, Parameter, RowField};
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let span = crate::source::Span::new(crate::source::SourceId::new(0), 0, 1);
        for row_member in [false, true] {
            let mut graph = InferenceContext::default();
            let first = graph.fresh(1, span).unwrap();
            let second = if row_member {
                let tail = graph.fresh_row(1, span).unwrap();
                let int = graph.atom(Atom::Int).unwrap();
                let row = graph.row(vec![RowField { label: Name::intern("tag"), ty: int }], Some(tail)).unwrap();
                graph.record(row).unwrap()
            } else { graph.fresh(1, span).unwrap() };
            let roots = [first, second].map(|ty| graph.arrow(Arrow {
                kind: CallableKind::Pure,
                params: vec![Parameter { label: Name::intern("value"), ty, defaulted: false, rest: false }],
                result: ty, effects: EffectSummary::Closed(EffectSet::EMPTY),
            }).unwrap());
            let members = roots.map(|root| ComponentMember { root, requirements: Vec::new(), policy: Generalization::Allowed });
            let schemes = graph.generalize_component(&members, 0, None).unwrap();
            let second = graph.resolved(second).unwrap();
            let mut pools = SemanticPools::default();
            let mut semantic = SemanticPoolBuilder::default();
            let mut builder = GenericEvidenceBuilder::default();
            let prepared = builder.prepare_reference(&graph, schemes[1], second, &mut pools, &mut semantic).unwrap();
            if row_member {
                let TypeRef::Template(template) = prepared else { panic!("row member must retain a template"); };
                let TypeTemplate::Record { row_tail, .. } = builder.template(template).unwrap() else { panic!("row member must retain a record"); };
                assert_eq!(*row_tail, Some(0));
            } else { assert_eq!(prepared, TypeRef::Rigid(0)); }
            let sibling = graph.scheme_type_binders(schemes[0]).unwrap()[0];
            assert!(builder.prepare_reference(&graph, schemes[1], sibling, &mut pools, &mut semantic).is_err());
        }
    }

    #[test]
    fn prepared_record_template_flattens_solved_field_extensions() {
        let source = "pure both(entry) { let _: Int = entry.age; entry.name }";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let declarations = crate::sema::check::Checker::check_compact_declarations(&parsed.arena);
        let bodies = crate::sema::check::Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        let declaration = bodies.solved.declarations.values().next().unwrap();
        let graph = &bodies.solved.graph;
        let crate::sema::inference::TypeNode::Arrow(arrow) = graph.node(graph.resolved(declaration.signature).unwrap()).unwrap() else { panic!("declaration must be callable"); };
        let mut pools = SemanticPools::default();
        let mut semantic = SemanticPoolBuilder::default();
        let mut builder = GenericEvidenceBuilder::default();
        let TypeRef::Template(template) = builder.prepare_reference(graph, declaration.scheme, arrow.params[0].ty, &mut pools, &mut semantic).unwrap() else { panic!("open record must retain a scoped template"); };
        let TypeTemplate::Record { fields, row_tail } = builder.template(template).unwrap() else { panic!("record parameter must retain its fields"); };
        let _symbols = parsed.arena.symbol_owner().enter();
        assert_eq!(fields.iter().map(|(name, _)| *name).collect::<Vec<_>>(), vec![Name::intern("age"), Name::intern("name")]);
        assert!(row_tail.is_some());
        let TypeRef::Ground(age) = fields[0].1 else { panic!("written age type must stay ground"); };
        assert_eq!(pools.to_type(age).unwrap(), Type::Int);
    }

    #[test]
    fn identity_evidence_retains_one_scope_for_distinct_payloads_and_fixed_return_plan() {
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default();
        let mut builder = GenericEvidenceBuilder::default(); let scope = builder.add_scope(identity(0)).unwrap();
        let mut instances = Vec::new();
        for ty in [Type::Int, Type::Str, Type::Bool, Type::Any] {
            let ty = scalar(&mut pools, &mut types, ty);
            instances.push(builder.add_instance(Instantiation { scope, substitutions: Box::new([ty]), parameter_types: Box::new([ty]), result_type: ty, requirements: Box::new([]) }).unwrap());
        }
        let store = builder.finish(&pools, 1, &[]).unwrap();
        assert_eq!(store.scopes.len(), 1);
        for instance in instances { assert_eq!(store.instance(instance).unwrap().scope, scope); }
        assert_eq!(store.scope(scope).unwrap().return_plan, GenericReturnPlan::Value);
    }

    #[test]
    fn foreign_retired_and_out_of_range_evidence_cannot_become_valid_by_slot_reuse() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut builder = GenericEvidenceBuilder::default(); let other = GenericEvidenceBuilder::default();
        assert!(builder.rewind(other.checkpoint()).is_err());
        let checkpoint = builder.checkpoint(); let retired = builder.add_scope(identity(0)).unwrap();
        let retired_template = builder.add_template(TypeTemplate::List(TypeRef::Rigid(0))).unwrap();
        builder.rewind(checkpoint).unwrap(); let replacement = builder.add_scope(identity(0)).unwrap();
        assert_eq!(retired.index, replacement.index); assert_ne!(retired.proof, replacement.proof);
        let replacement_template = builder.add_template(TypeTemplate::List(TypeRef::Rigid(0))).unwrap();
        assert_eq!(retired_template.index, replacement_template.index); assert_ne!(retired_template.proof, replacement_template.proof);
        assert!(builder.store.template(retired_template).is_err());
        assert!(builder.store.scope(retired).is_err()); assert!(other.store.scope(replacement).is_err());
        let malformed = SchemeScopeId { index: 9, ..replacement }; assert!(builder.store.scope(malformed).is_err());
    }

    #[test]
    fn projection_uses_constructor_layout_and_rejects_wrong_slot_type_and_owner() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default();
        let int = scalar(&mut pools, &mut types, Type::Int); let text = scalar(&mut pools, &mut types, Type::Str);
        let field = Name::intern("value"); let extra = Name::intern("extra");
        let record = scalar(&mut pools, &mut types, Type::Record(BTreeMap::from([(field, Type::Int), (extra, Type::Str)])));
        let mut builder = GenericEvidenceBuilder::default(); let mut body = identity(0); body.quantifiers = Box::new([QuantifierKind::Type, QuantifierKind::Type]); body.result = TypeRef::Rigid(1);
        body.requirements = Box::new([Requirement::Projection { receiver: TypeRef::Rigid(0), receiver_parameter: 0, field, result: TypeRef::Rigid(1) }]);
        let scope = builder.add_scope(body).unwrap();
        let layout = builder.add_layout(PhysicalLayout { record_type: record, fields: Box::new([(extra, text), (field, int)]) }).unwrap();
        let instance = builder.add_instance(Instantiation { scope, substitutions: Box::new([record, int]), parameter_types: Box::new([record]), result_type: int, requirements: Box::new([RequirementWitness::Projection { layout, field_slot: 1, result: int }]) }).unwrap();
        builder.add_requirement_use(SolvedRequirementUse { instruction: 0, scope, requirement: 0 });
        builder.add_constructor(SolvedRecordLayout { instruction: 1, owner: InstructionOwner::Function(IrFunctionId::new(0).unwrap()), layout });
        let store = builder.finish(&pools, 1, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap())); 2]).unwrap();
        assert!(matches!(store.instance(instance).unwrap().requirements[0], RequirementWitness::Projection { field_slot: 1, .. }));
        for slot in [0, 2] {
            let mut bad = store.clone(); bad.instances[instance.index as usize].value.requirements[0] = RequirementWitness::Projection { layout, field_slot: slot, result: int };
            assert!(bad.verify(&pools, 1, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap())); 2]).is_err());
        }
        let mut bad = store.clone(); bad.layouts[layout.index as usize].value.fields[1].1 = text;
        assert!(bad.verify(&pools, 1, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap())); 2]).is_err());
        assert!(store.verify(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(1).unwrap())); 2]).is_err());
    }

    #[test]
    fn concrete_add_witnesses_reject_foreign_signatures_and_erased_repairs() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        for (ty, operation, accepted) in [(Type::Int, ConcreteOperationId::AddInt, true), (Type::Float, ConcreteOperationId::AddFloat, true), (Type::Str, ConcreteOperationId::AddStr, true), (Type::Bool, ConcreteOperationId::AddInt, false), (Type::Int, ConcreteOperationId::AddStr, false), (Type::Any, ConcreteOperationId::AddInt, false)] {
            let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default(); let ty = scalar(&mut pools, &mut types, ty);
            let mut builder = GenericEvidenceBuilder::default(); let scope = builder.add_scope(addition(0)).unwrap();
            builder.add_instance(Instantiation { scope, substitutions: Box::new([ty]), parameter_types: Box::new([ty, ty]), result_type: ty, requirements: Box::new([RequirementWitness::Add { operation, left: ty, right: ty, result: ty }]) }).unwrap();
            assert_eq!(builder.finish(&pools, 1, &[]).is_ok(), accepted);
        }
    }

    #[test]
    fn forwarding_precomputes_scoped_edges_and_rejects_missing_or_ambiguous_maps() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default(); let int = scalar(&mut pools, &mut types, Type::Int);
        let mut builder = GenericEvidenceBuilder::default(); let caller = builder.add_scope(addition(0)).unwrap(); let callee = builder.add_scope(addition(1)).unwrap();
        let witness = RequirementWitness::Add { operation: ConcreteOperationId::AddInt, left: int, right: int, result: int };
        let from = builder.add_instance(Instantiation { scope: caller, substitutions: Box::new([int]), parameter_types: Box::new([int, int]), result_type: int, requirements: Box::new([witness]) }).unwrap();
        let to = builder.add_instance(Instantiation { scope: callee, substitutions: Box::new([int]), parameter_types: Box::new([int, int]), result_type: int, requirements: Box::new([witness]) }).unwrap();
        let forwarding = builder.add_forwarding(ForwardingPlan { caller, callee, substitutions: Box::new([TypeRef::Rigid(0)]), requirements: Box::new([ForwardedRequirement::Caller(0)]), instances: Box::new([(from, to)]) }).unwrap();
        builder.add_call(SolvedCall { instruction: 0, caller: InstructionOwner::Function(IrFunctionId::new(0).unwrap()), target: callee, evidence: CallEvidence::Forwarded(forwarding) });
        for parameter in 0..2 { builder.add_argument(SolvedArgument { call_instruction: 0, parameter, source_instruction: None, ty: TypeRef::Rigid(0) }); }
        let store = builder.finish(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap()))]).unwrap();
        assert_eq!(store.forwarded_instance(forwarding, from).unwrap(), to); assert!(store.forwarded_instance(forwarding, to).is_err());
        let mut bad = store.clone(); bad.forwarding[forwarding.index as usize].value.requirements[0] = ForwardedRequirement::Caller(1); assert!(bad.verify(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap()))]).is_err());
        let mut bad = store.clone(); bad.forwarding[forwarding.index as usize].value.instances = Box::new([(from, to), (from, to)]); assert!(bad.verify(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap()))]).is_err());
        let mut bad = store.clone(); bad.calls.push(store.calls[0]); assert!(bad.verify(&pools, 2, &[Some(InstructionOwner::Function(IrFunctionId::new(0).unwrap()))]).is_err());
    }
    #[test]
    fn row_tail_substitution_preserves_width_and_explicit_dynamic_fields() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default();
        let name = Name::intern("name"); let extra = Name::intern("extra");
        let any = scalar(&mut pools, &mut types, Type::Any);
        let bool_type = scalar(&mut pools, &mut types, Type::Bool);
        let tail = scalar(&mut pools, &mut types, Type::Record(BTreeMap::from([(extra, Type::Bool)])));
        let record = scalar(&mut pools, &mut types, Type::Record(BTreeMap::from([(name, Type::Any), (extra, Type::Bool)])));
        let mut builder = GenericEvidenceBuilder::default();
        let receiver = TypeRef::Template(builder.add_template(TypeTemplate::Record { fields: Box::new([(name, TypeRef::Rigid(0))]), row_tail: Some(1) }).unwrap());
        let mut declaration = identity(0);
        declaration.quantifiers = Box::new([QuantifierKind::Type, QuantifierKind::Row]);
        declaration.parameters = Box::new([receiver]);
        declaration.requirements = Box::new([Requirement::Projection { receiver, receiver_parameter: 0, field: name, result: TypeRef::Rigid(0) }]);
        let scope = builder.add_scope(declaration).unwrap();
        let layout = builder.add_layout(PhysicalLayout { record_type: record, fields: Box::new([(name, any), (extra, bool_type)]) }).unwrap();
        let proof = Instantiation { scope, substitutions: Box::new([any, tail]), parameter_types: Box::new([record]), result_type: any, requirements: Box::new([RequirementWitness::Projection { layout, field_slot: 0, result: any }]) };
        let instance = builder.add_instance(proof.clone()).unwrap();
        assert_eq!(builder.add_instance(proof).unwrap(), instance);
        let store = builder.finish(&pools, 1, &[]).unwrap();
        assert_eq!(store.instances.len(), 1);
        assert_eq!(pools.to_type(store.instance(instance).unwrap().result_type).unwrap(), Type::Any);
        let mut whole_record_as_tail = store.clone(); whole_record_as_tail.instances[instance.index as usize].value.substitutions[1] = record;
        assert!(whole_record_as_tail.verify(&pools, 1, &[]).is_err());
        let mut wrong_kind = store.clone(); wrong_kind.scopes[scope.index as usize].value.quantifiers[1] = QuantifierKind::Type;
        assert!(wrong_kind.verify(&pools, 1, &[]).is_err());
        let mut erased_receiver = store.clone();
        let erased = scalar(&mut pools, &mut types, Type::ErasedRecord);
        erased_receiver.instances[instance.index as usize].value.parameter_types[0] = erased;
        assert!(erased_receiver.verify(&pools, 1, &[]).is_err());
    }

    #[test]
    fn nested_templates_and_any_substitution_cannot_launder_concrete_arguments() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut pools = SemanticPools::default(); let mut types = SemanticPoolBuilder::default();
        let any = scalar(&mut pools, &mut types, Type::Any);
        let int = scalar(&mut pools, &mut types, Type::Int);
        let error = scalar(&mut pools, &mut types, Type::Error);
        let input = Type::List(Box::new(Type::Optional(Box::new(Type::Any))));
        let argument = scalar(&mut pools, &mut types, input.clone());
        let result_type = scalar(&mut pools, &mut types, Type::Result(Box::new(input), Box::new(Type::Error)));
        let mut builder = GenericEvidenceBuilder::default();
        let optional = TypeRef::Template(builder.add_template(TypeTemplate::Optional(TypeRef::Rigid(0))).unwrap());
        let list = TypeRef::Template(builder.add_template(TypeTemplate::List(optional)).unwrap());
        let result = TypeRef::Template(builder.add_template(TypeTemplate::Result { ok: list, error: TypeRef::Ground(error) }).unwrap());
        let mut declaration = identity(0); declaration.parameters = Box::new([list]); declaration.result = result; declaration.return_plan = GenericReturnPlan::Result;
        let scope = builder.add_scope(declaration).unwrap();
        let instance = builder.add_instance(Instantiation { scope, substitutions: Box::new([any]), parameter_types: Box::new([argument]), result_type, requirements: Box::new([]) }).unwrap();
        let store = builder.finish(&pools, 1, &[]).unwrap();
        let mut wrong_argument = store.clone(); wrong_argument.instances[instance.index as usize].value.parameter_types[0] = scalar(&mut pools, &mut types, Type::List(Box::new(Type::Optional(Box::new(Type::Int)))));
        assert!(wrong_argument.verify(&pools, 1, &[]).is_err());
        let mut wrong_result = store.clone(); wrong_result.instances[instance.index as usize].value.result_type = int;
        assert!(wrong_result.verify(&pools, 1, &[]).is_err());
        let mut foreign_template = store.clone(); let mut outsider = GenericEvidenceBuilder::default();
        foreign_template.scopes[scope.index as usize].value.result = TypeRef::Template(outsider.add_template(TypeTemplate::List(TypeRef::Ground(any))).unwrap());
        assert!(foreign_template.verify(&pools, 1, &[]).is_err());
    }

}

fn verify_instruction_order(instructions: impl Iterator<Item = u32>) -> Result<(), IrVerifyError> {
    let mut previous = None;
    for instruction in instructions {
        if previous.is_some_and(|previous| previous >= instruction) { return Err(failure("solved generic instruction map is not unique and sorted")); }
        previous = Some(instruction);
    }
    Ok(())
}

fn parameter_accepts(formal: &crate::sema::types::Type, actual: &crate::sema::types::Type) -> bool {
    use crate::sema::types::Type;
    match (formal, actual) {
        (Type::Record(expected), Type::Record(fields)) => expected.iter().all(|(name, ty)| fields.get(name) == Some(ty)),
        _ => formal == actual,
    }
}

fn contains_unresolved(ty: &crate::sema::types::Type) -> bool {
    use crate::sema::types::Type;
    match ty {
        Type::Unknown | Type::Invalid | Type::Inference(_) | Type::Graph(_) | Type::BuiltinParameter(_) => true,
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => contains_unresolved(inner),
        Type::Map(left, right) | Type::Result(left, right) => contains_unresolved(left) || contains_unresolved(right),
        Type::Record(fields) => fields.values().any(contains_unresolved),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct GenericCheckpoint { root: u64, serial_limit: u64, scopes: usize, layouts: usize, instances: usize, forwarding: usize, templates: usize, calls: usize, arguments: usize, uses: usize, constructors: usize }

pub(in crate::runtime::eval) struct GenericEvidenceBuilder { store: GenericEvidenceStore, next_serial: u64, canonical_templates: rustc_hash::FxHashMap<TypeTemplate, TypeTemplateId>, canonical_instances: rustc_hash::FxHashMap<Instantiation, InstantiationId> }

impl Default for GenericEvidenceBuilder {
    fn default() -> Self {
        let root = NEXT_ROOT.try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1)).expect("generic proof roots exhausted");
        Self { store: GenericEvidenceStore { root, scopes: Vec::new(), layouts: Vec::new(), instances: Vec::new(), forwarding: Vec::new(), templates: Vec::new(), calls: Vec::new(), arguments: Vec::new(), uses: Vec::new(), constructors: Vec::new(), function_scopes: Vec::new() }, next_serial: 1, canonical_templates: rustc_hash::FxHashMap::default(), canonical_instances: rustc_hash::FxHashMap::default() }
    }
}

macro_rules! insert_evidence {
    ($visibility:vis $method:ident, $field:ident, $ty:ty, $id:ident) => {
        $visibility fn $method(&mut self, value: $ty) -> Result<$id, IrVerifyError> {
            let index = u32::try_from(self.store.$field.len()).map_err(|_| failure("generic evidence id overflow"))?;
            let serial = self.next_serial;
            self.next_serial = serial.checked_add(1).ok_or_else(|| failure("generic evidence serial overflow"))?;
            self.store.$field.push(Entry { serial, value });
            Ok($id { index, proof: OwnerProof { root: self.store.root, serial } })
        }
    };
}

impl GenericEvidenceBuilder {
    insert_evidence!(pub add_scope, scopes, SchemeScope, SchemeScopeId);
    insert_evidence!(pub add_layout, layouts, PhysicalLayout, PhysicalLayoutId);
    insert_evidence!(add_instance_uncached, instances, Instantiation, InstantiationId);
    insert_evidence!(pub add_forwarding, forwarding, ForwardingPlan, ForwardingId);
    insert_evidence!(add_template_uncached, templates, TypeTemplate, TypeTemplateId);
    pub fn add_instance(&mut self, instance: Instantiation) -> Result<InstantiationId, IrVerifyError> {
        if let Some(&id) = self.canonical_instances.get(&instance) { return Ok(id); }
        let id = self.add_instance_uncached(instance.clone())?;
        self.canonical_instances.insert(instance, id);
        Ok(id)
    }
    pub fn add_template(&mut self, mut template: TypeTemplate) -> Result<TypeTemplateId, IrVerifyError> {
        if let TypeTemplate::Record { fields, .. } = &mut template { fields.sort_unstable_by_key(|field| field.0); }
        if let Some(&id) = self.canonical_templates.get(&template) { return Ok(id); }
        let id = self.add_template_uncached(template.clone())?;
        self.canonical_templates.insert(template, id);
        Ok(id)
    }
    pub fn add_call(&mut self, call: SolvedCall) { self.store.calls.push(call); }
    pub fn add_argument(&mut self, argument: SolvedArgument) { self.store.arguments.push(argument); }
    pub fn add_requirement_use(&mut self, use_: SolvedRequirementUse) { self.store.uses.push(use_); }
    pub fn add_constructor(&mut self, record: SolvedRecordLayout) { self.store.constructors.push(record); }
    pub fn checkpoint(&self) -> GenericCheckpoint { GenericCheckpoint { root: self.store.root, serial_limit: self.next_serial, scopes: self.store.scopes.len(), layouts: self.store.layouts.len(), instances: self.store.instances.len(), forwarding: self.store.forwarding.len(), templates: self.store.templates.len(), calls: self.store.calls.len(), arguments: self.store.arguments.len(), uses: self.store.uses.len(), constructors: self.store.constructors.len() } }
    pub fn rewind(&mut self, checkpoint: GenericCheckpoint) -> Result<(), IrVerifyError> {
        if checkpoint.root != self.store.root { return Err(failure("generic checkpoint belongs to a foreign program")); }
        if checkpoint.scopes > self.store.scopes.len() || checkpoint.layouts > self.store.layouts.len() || checkpoint.instances > self.store.instances.len() || checkpoint.forwarding > self.store.forwarding.len() || checkpoint.templates > self.store.templates.len() || checkpoint.calls > self.store.calls.len() || checkpoint.arguments > self.store.arguments.len() || checkpoint.uses > self.store.uses.len() || checkpoint.constructors > self.store.constructors.len() { return Err(failure("generic checkpoint references retired entries")); }
        for serial in [self.store.scopes.get(checkpoint.scopes.wrapping_sub(1)).map(|entry| entry.serial), self.store.layouts.get(checkpoint.layouts.wrapping_sub(1)).map(|entry| entry.serial), self.store.instances.get(checkpoint.instances.wrapping_sub(1)).map(|entry| entry.serial), self.store.forwarding.get(checkpoint.forwarding.wrapping_sub(1)).map(|entry| entry.serial), self.store.templates.get(checkpoint.templates.wrapping_sub(1)).map(|entry| entry.serial)].into_iter().flatten() {
            if serial >= checkpoint.serial_limit { return Err(failure("generic checkpoint references replacement entries")); }
        }
        self.store.scopes.truncate(checkpoint.scopes); self.store.layouts.truncate(checkpoint.layouts); self.store.instances.truncate(checkpoint.instances); self.store.forwarding.truncate(checkpoint.forwarding); self.store.templates.truncate(checkpoint.templates); self.store.calls.truncate(checkpoint.calls); self.store.arguments.truncate(checkpoint.arguments); self.store.uses.truncate(checkpoint.uses); self.store.constructors.truncate(checkpoint.constructors);
        self.canonical_templates.retain(|_, id| id.index < checkpoint.templates as u32);
        self.canonical_instances.retain(|_, id| id.index < checkpoint.instances as u32);
        Ok(())
    }
    pub(super) fn finish(mut self, pools: &SemanticPools, function_count: usize, instruction_owners: &[Option<InstructionOwner>]) -> Result<GenericEvidenceStore, IrVerifyError> {
        self.store.arguments.sort_unstable_by_key(|argument| (argument.call_instruction, argument.parameter));
        self.store.calls.sort_by_key(|call| call.instruction); self.store.uses.sort_by_key(|use_| use_.instruction); self.store.constructors.sort_by_key(|record| record.instruction);
        self.store.function_scopes = self.store.scopes().map(|(id, scope)| (scope.owner, id)).collect();
        self.store.function_scopes.sort_by_key(|entry| entry.0.raw());
        for entry in &mut self.store.forwarding { entry.value.instances.sort_unstable_by_key(|edge| edge.0.index); }
        self.store.verify(pools, function_count, instruction_owners)?;
        Ok(self.store)
    }
}

pub(super) fn graph_ground_type(graph: &crate::sema::inference::InferenceContext, id: crate::sema::inference::TypeId) -> Result<crate::sema::types::Type, IrVerifyError> {
    fn ground(graph: &crate::sema::inference::InferenceContext, id: crate::sema::inference::TypeId, active: &mut Vec<crate::sema::inference::TypeId>) -> Result<crate::sema::types::Type, IrVerifyError> {
        use crate::sema::inference::{Atom, TypeNode};
        use crate::sema::types::Type;
        let id = graph.resolved(id).map_err(|_| failure("graph type handle is foreign or unresolved"))?;
        if active.len() >= 256 || active.contains(&id) { return Err(failure("graph ground type is cyclic or exceeds depth limit")); }
        active.push(id);
        let ty = match graph.node(id).map_err(|_| failure("graph type handle is foreign"))? {
            TypeNode::Atom(atom) => match *atom {
                Atom::Any => Type::Any, Atom::ErasedRecord => Type::ErasedRecord, Atom::DynamicModule => Type::DynamicModule, Atom::Pure => Type::Pure, Atom::Proc => Type::Proc, Atom::Command => Type::Command,
                Atom::Null => Type::Null, Atom::Bool => Type::Bool, Atom::Int => Type::Int, Atom::UInt => Type::UInt, Atom::Float => Type::Float, Atom::Duration => Type::Duration, Atom::Str => Type::Str, Atom::Bytes => Type::Bytes, Atom::Digest => Type::Digest, Atom::Regex => Type::Regex, Atom::Path => Type::Path,
                Atom::Unit => Type::Unit, Atom::Status => Type::Status, Atom::EnvPathList => Type::EnvPathList, Atom::Error => Type::Error, Atom::ProcessError => Type::ProcessError, Atom::ProcessHandle => Type::ProcessHandle, Atom::NetJob => Type::NetJob, Atom::FsRoot => Type::FsRoot,
                Atom::Tag(name) => Type::Tag(name), Atom::ErrorFamily(name) => Type::ErrorFamily(name), Atom::ErrorVariant { family, variant } => Type::ErrorVariant { family, variant }, Atom::ErrorFacet(name) => Type::ErrorFacet(name),
            },
            TypeNode::Optional(inner) => Type::Optional(Box::new(ground(graph, *inner, active)?)),
            TypeNode::List(inner) => Type::List(Box::new(ground(graph, *inner, active)?)),
            TypeNode::Stream(inner) => Type::Stream(Box::new(ground(graph, *inner, active)?)),
            TypeNode::Map(key, value) => Type::Map(Box::new(ground(graph, *key, active)?), Box::new(ground(graph, *value, active)?)),
            TypeNode::Result(ok, error) => Type::Result(Box::new(ground(graph, *ok, active)?), Box::new(ground(graph, *error, active)?)),
            TypeNode::Record(row) | TypeNode::Row(row) => {
                let row = graph.row_data(*row).map_err(|_| failure("graph row handle is foreign"))?;
                let mut fields = std::collections::BTreeMap::new();
                for field in &row.fields { if fields.insert(field.label, ground(graph, field.ty, active)?).is_some() { return Err(failure("graph row repeats a field")); } }
                if let Some(tail) = row.tail {
                    let Type::Record(tail) = ground(graph, tail, active)? else { return Err(failure("graph row tail is not a closed row")); };
                    for (name, ty) in tail { if fields.insert(name, ty).is_some() { return Err(failure("graph row tail violates a lacks constraint")); } }
                }
                Type::Record(fields)
            }
            _ => return Err(failure("graph type is not closed ground data")),
        };
        active.pop();
        Ok(ty)
    }
    ground(graph, id, &mut Vec::new())
}

impl GenericEvidenceBuilder {
    pub(super) fn prepare_reference(&mut self, graph: &crate::sema::inference::InferenceContext, scheme: crate::sema::inference::SchemeId, id: crate::sema::inference::TypeId, pools: &mut SemanticPools, semantic: &mut super::semantic::SemanticPoolBuilder) -> Result<TypeRef, IrVerifyError> {
        fn prepare(builder: &mut GenericEvidenceBuilder, graph: &crate::sema::inference::InferenceContext, scheme: crate::sema::inference::SchemeId, id: crate::sema::inference::TypeId, pools: &mut SemanticPools, semantic: &mut super::semantic::SemanticPoolBuilder, active: &mut Vec<crate::sema::inference::TypeId>) -> Result<TypeRef, IrVerifyError> {
            use crate::sema::inference::{TypeNode, VariableKind};
            let id = graph.resolved(id).map_err(|_| failure("generic template graph handle is invalid"))?;
            if active.len() >= 256 || active.contains(&id) { return Err(failure("generic template graph is cyclic or exceeds depth limit")); }
            if let Ok(ty) = graph_ground_type(graph, id) { return semantic.intern_type(pools, &ty).map(TypeRef::Ground).map_err(|_| failure("ground graph data cannot enter the semantic pool")); }
            active.push(id);
            let reference = match graph.node(id).map_err(|_| failure("generic template graph handle is foreign"))? {
                TypeNode::Rigid { kind: VariableKind::Type, .. } => {
                    let index = graph.scheme_binder_index(scheme, id).map_err(|_| failure("generic template scheme is foreign"))?
                        .ok_or_else(|| failure("generic template variable is outside the member scheme"))?;
                    TypeRef::Rigid(u32::try_from(index).map_err(|_| failure("generic template quantifier index exceeds its representation"))?)
                },
                TypeNode::Optional(inner) => { let inner = prepare(builder, graph, scheme, *inner, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::Optional(inner))?) },
                TypeNode::List(inner) => { let inner = prepare(builder, graph, scheme, *inner, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::List(inner))?) },
                TypeNode::Stream(inner) => { let inner = prepare(builder, graph, scheme, *inner, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::Stream(inner))?) },
                TypeNode::Map(key, value) => { let key = prepare(builder, graph, scheme, *key, pools, semantic, active)?; let value = prepare(builder, graph, scheme, *value, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::Map { key, value })?) },
                TypeNode::Result(ok, error) => { let ok = prepare(builder, graph, scheme, *ok, pools, semantic, active)?; let error = prepare(builder, graph, scheme, *error, pools, semantic, active)?; TypeRef::Template(builder.add_template(TypeTemplate::Result { ok, error })?) },
                TypeNode::Record(row) | TypeNode::Row(row) => {
                    let mut row = *row;
                    let mut fields = std::collections::BTreeMap::new();
                    let mut tails = Vec::new();
                    let row_tail = loop {
                        let data = graph.row_data(row).map_err(|_| failure("generic template row is foreign"))?;
                        for field in &data.fields {
                            let ty = prepare(builder, graph, scheme, field.ty, pools, semantic, active)?;
                            if fields.insert(field.label, ty).is_some() { return Err(failure("generic row extension violates a lacks constraint")); }
                        }
                        let Some(tail) = data.tail else { break None; };
                        let tail = graph.resolved(tail).map_err(|_| failure("generic row tail is unresolved"))?;
                        if active.len() + tails.len() >= 256 || active.contains(&tail) || tails.contains(&tail) { return Err(failure("generic row tail is cyclic or exceeds depth limit")); }
                        match graph.node(tail).map_err(|_| failure("generic row tail is foreign"))? {
                            TypeNode::Rigid { kind: VariableKind::Row, .. } => {
                                let index = graph.scheme_binder_index(scheme, tail).map_err(|_| failure("generic template scheme is foreign"))?
                                    .ok_or_else(|| failure("generic row variable is outside the member scheme"))?;
                                break Some(u32::try_from(index).map_err(|_| failure("generic row quantifier index exceeds its representation"))?);
                            },
                            TypeNode::Record(next) | TypeNode::Row(next) => { tails.push(tail); row = *next; },
                            _ => return Err(failure("generic row tail is not a scoped row variable or row extension")),
                        }
                    };
                    TypeRef::Template(builder.add_template(TypeTemplate::Record { fields: fields.into_iter().collect::<Vec<_>>().into_boxed_slice(), row_tail })?)
                }
                _ => return Err(failure("generic graph node lacks a prepared scoped representation")),
            };
            active.pop();
            Ok(reference)
        }
        prepare(self, graph, scheme, id, pools, semantic, &mut Vec::new())
    }
    pub(super) fn scope(&self, id: SchemeScopeId) -> Result<&SchemeScope, IrVerifyError> { self.store.scope(id) }
    pub(super) fn instance(&self, id: InstantiationId) -> Result<&Instantiation, IrVerifyError> { self.store.instance(id) }
    pub(super) fn layout(&self, id: PhysicalLayoutId) -> Result<&PhysicalLayout, IrVerifyError> { self.store.layout(id) }
    pub(super) fn forwarding(&self, id: ForwardingId) -> Result<&ForwardingPlan, IrVerifyError> { self.store.forwarding(id) }
    pub(super) fn template(&self, id: TypeTemplateId) -> Result<&TypeTemplate, IrVerifyError> { self.store.template(id) }
    pub(super) fn scopes(&self) -> impl Iterator<Item = (SchemeScopeId, &SchemeScope)> { self.store.scopes() }
    pub(super) fn instances(&self) -> impl Iterator<Item = (InstantiationId, &Instantiation)> {
        self.store.instances.iter().enumerate().map(|(index, entry)| (InstantiationId { index: index as u32, proof: OwnerProof { root: self.store.root, serial: entry.serial } }, &entry.value))
    }
    pub(super) fn materialize_reference(&self, reference: TypeRef, substitutions: &[GroundTypeId], pools: &mut SemanticPools, semantic: &mut super::semantic::SemanticPoolBuilder) -> Result<GroundTypeId, IrVerifyError> {
        let ty = self.store.expand(pools, reference, substitutions, &mut rustc_hash::FxHashMap::default())?;
        semantic.intern_type(pools, &ty).map_err(|_| failure("instantiated template cannot enter the ground pool"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NormalizedType {
    Scalar(crate::sema::types::Type),
    Rigid(u32, QuantifierKind),
    Optional(Box<Self>), List(Box<Self>), Stream(Box<Self>),
    Map(Box<Self>, Box<Self>), Result(Box<Self>, Box<Self>),
    Record(std::collections::BTreeMap<Name, Self>, Option<u32>),
    Arrow(CallableKind, Vec<(Name, ParameterMode, bool, bool, Self)>, Box<Self>, Box<[crate::syntax::node::Effect]>),
}

fn normalize_ground(ty: crate::sema::types::Type) -> NormalizedType {
    use crate::sema::types::Type;
    match ty {
        Type::Optional(inner) => NormalizedType::Optional(Box::new(normalize_ground(*inner))),
        Type::List(inner) => NormalizedType::List(Box::new(normalize_ground(*inner))),
        Type::Stream(inner) => NormalizedType::Stream(Box::new(normalize_ground(*inner))),
        Type::Map(key, value) => NormalizedType::Map(Box::new(normalize_ground(*key)), Box::new(normalize_ground(*value))),
        Type::Result(ok, error) => NormalizedType::Result(Box::new(normalize_ground(*ok)), Box::new(normalize_ground(*error))),
        Type::Record(fields) => NormalizedType::Record(fields.into_iter().map(|(name, ty)| (name, normalize_ground(ty))).collect(), None),
        scalar => NormalizedType::Scalar(scalar),
    }
}

impl GenericEvidenceStore {
    fn normalized(&self, pools: &SemanticPools, scope: &SchemeScope, reference: TypeRef, substitutions: Option<&[NormalizedType]>, active: &mut Vec<TypeTemplateId>) -> Result<NormalizedType, IrVerifyError> {
        if active.len() >= 256 { return Err(failure("symbolic template exceeds depth limit")); }
        Ok(match reference {
            TypeRef::Ground(ty) => normalize_ground(pools.to_type(ty)?),
            TypeRef::Rigid(index) => {
                let kind = *scope.quantifiers.get(index as usize).ok_or_else(|| failure("symbolic rigid is out of scope"))?;
                if let Some(substitutions) = substitutions { substitutions.get(index as usize).cloned().ok_or_else(|| failure("symbolic substitution is out of scope"))? } else { NormalizedType::Rigid(index, kind) }
            }
            TypeRef::Template(id) => {
                if active.contains(&id) { return Err(failure("symbolic template contains a cycle")); }
                active.push(id);
                let result = match self.template(id)? {
                    TypeTemplate::Optional(inner) => NormalizedType::Optional(Box::new(self.normalized(pools, scope, *inner, substitutions, active)?)),
                    TypeTemplate::List(inner) => NormalizedType::List(Box::new(self.normalized(pools, scope, *inner, substitutions, active)?)),
                    TypeTemplate::Stream(inner) => NormalizedType::Stream(Box::new(self.normalized(pools, scope, *inner, substitutions, active)?)),
                    TypeTemplate::Map { key, value } => NormalizedType::Map(Box::new(self.normalized(pools, scope, *key, substitutions, active)?), Box::new(self.normalized(pools, scope, *value, substitutions, active)?)),
                    TypeTemplate::Result { ok, error } => NormalizedType::Result(Box::new(self.normalized(pools, scope, *ok, substitutions, active)?), Box::new(self.normalized(pools, scope, *error, substitutions, active)?)),
                    TypeTemplate::Record { fields, row_tail } => {
                        let mut fields = fields.iter().map(|&(name, ty)| self.normalized(pools, scope, ty, substitutions, active).map(|ty| (name, ty))).collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
                        let mut tail = None;
                        if let Some(index) = row_tail {
                            if scope.quantifiers.get(*index as usize) != Some(&QuantifierKind::Row) { return Err(failure("symbolic record tail has the wrong quantifier kind")); }
                            let row = if let Some(substitutions) = substitutions { substitutions.get(*index as usize).cloned().ok_or_else(|| failure("symbolic row substitution is missing"))? } else { NormalizedType::Rigid(*index, QuantifierKind::Row) };
                            match row {
                                NormalizedType::Rigid(index, QuantifierKind::Row) => tail = Some(index),
                                NormalizedType::Record(row, row_tail) => { for (name, ty) in row { if fields.insert(name, ty).is_some() { return Err(failure("symbolic row substitution violates a lacks constraint")); } } tail = row_tail; }
                                _ => return Err(failure("symbolic row substitution is not a row")),
                            }
                        }
                        NormalizedType::Record(fields, tail)
                    }
                    TypeTemplate::Arrow { kind, parameters, result, effects } => {
                        let parameters = parameters.iter().map(|parameter| self.normalized(pools, scope, parameter.ty, substitutions, active).map(|ty| (parameter.label, parameter.mode, parameter.defaulted, parameter.rest, ty))).collect::<Result<Vec<_>, _>>()?;
                        NormalizedType::Arrow(*kind, parameters, Box::new(self.normalized(pools, scope, *result, substitutions, active)?), effects.clone())
                    }
                };
                active.pop();
                result
            }
        })
    }

    fn verify_return_plan(&self, pools: &SemanticPools, scope: &SchemeScope) -> Result<(), IrVerifyError> {
        use crate::sema::types::Type;
        let result = self.normalized(pools, scope, scope.result, None, &mut Vec::new())?;
        let valid = match scope.return_plan {
            GenericReturnPlan::Value => true,
            GenericReturnPlan::Result => matches!(result, NormalizedType::Result(_, _)),
            GenericReturnPlan::Unit => result == NormalizedType::Scalar(Type::Unit),
            GenericReturnPlan::ResultUnit => matches!(result, NormalizedType::Result(ok, _) if *ok == NormalizedType::Scalar(Type::Unit)),
        };
        if valid { Ok(()) } else { Err(failure("generic return plan disagrees with its formal result shape")) }
    }

    fn verify_symbolic_forwarding(&self, pools: &SemanticPools, plan: &ForwardingPlan) -> Result<(), IrVerifyError> {
        let caller = self.scope(plan.caller)?; let callee = self.scope(plan.callee)?;
        let substitutions = plan.substitutions.iter().map(|&reference| self.normalized(pools, caller, reference, None, &mut Vec::new())).collect::<Result<Vec<_>, _>>()?;
        for (requirement, evidence) in callee.requirements.iter().zip(&plan.requirements) {
            let normalize = |reference| self.normalized(pools, callee, reference, Some(&substitutions), &mut Vec::new());
            match evidence {
                ForwardedRequirement::Caller(index) => {
                    let source = caller.requirements.get(*index as usize).ok_or_else(|| failure("symbolic forwarded requirement is out of scope"))?;
                    let caller_type = |reference| self.normalized(pools, caller, reference, None, &mut Vec::new());
                    let valid = match (requirement, source) {
                        (Requirement::Add { left, right, result }, Requirement::Add { left: source_left, right: source_right, result: source_result }) => normalize(*left)? == caller_type(*source_left)? && normalize(*right)? == caller_type(*source_right)? && normalize(*result)? == caller_type(*source_result)?,
                        (Requirement::Projection { receiver, field, result, .. }, Requirement::Projection { receiver: source_receiver, field: source_field, result: source_result, .. }) => field == source_field && normalize(*receiver)? == caller_type(*source_receiver)? && normalize(*result)? == caller_type(*source_result)?,
                        _ => false,
                    };
                    if !valid { return Err(failure("forwarded requirement disagrees with the symbolic declaration map")); }
                }
                ForwardedRequirement::Fixed(witness) => match (requirement, *witness) {
                    (Requirement::Add { left, right, result }, RequirementWitness::Add { operation, left: actual_left, right: actual_right, result: actual_result }) => {
                        if normalize(*left)? != normalize_ground(pools.to_type(actual_left)?) || normalize(*right)? != normalize_ground(pools.to_type(actual_right)?) || normalize(*result)? != normalize_ground(pools.to_type(actual_result)?) { return Err(failure("fixed operation evidence cannot discharge a varying symbolic requirement")); }
                        let valid = match operation { ConcreteOperationId::AddInt => matches!(pools.type_tag(actual_left)?, TypeTag::Int | TypeTag::UInt) && matches!(pools.type_tag(actual_right)?, TypeTag::Int | TypeTag::UInt) && pools.type_tag(actual_result)? == TypeTag::Int, ConcreteOperationId::AddFloat => pools.type_tag(actual_left)? == TypeTag::Float && pools.type_tag(actual_right)? == TypeTag::Float && pools.type_tag(actual_result)? == TypeTag::Float, ConcreteOperationId::AddStr => pools.type_tag(actual_left)? == TypeTag::Str && pools.type_tag(actual_right)? == TypeTag::Str && pools.type_tag(actual_result)? == TypeTag::Str };
                        if !valid { return Err(failure("fixed operation evidence has an unsupported concrete signature")); }
                    }
                    (Requirement::Projection { receiver, field, result, .. }, RequirementWitness::Projection { layout, field_slot, result: actual_result }) => {
                        let layout = self.layout(layout)?;
                        let (actual_field, ty) = *layout.fields.get(field_slot as usize).ok_or_else(|| failure("fixed projection slot is out of bounds"))?;
                        let actual_receiver = normalize_ground(pools.to_type(layout.record_type)?);
                        let expected_receiver = normalize(*receiver)?;
                        let compatible = match (&expected_receiver, &actual_receiver) { (NormalizedType::Record(expected, None), NormalizedType::Record(actual, None)) => expected.iter().all(|(name, ty)| actual.get(name) == Some(ty)), _ => expected_receiver == actual_receiver };
                        if actual_field != *field || ty != actual_result || !compatible || normalize(*result)? != normalize_ground(pools.to_type(actual_result)?) { return Err(failure("fixed projection evidence disagrees with its symbolic requirement")); }
                    }
                    _ => return Err(failure("fixed forwarding witness has the wrong requirement kind")),
                },
            }
        }
        Ok(())
    }
}

impl GenericEvidenceStore {
    pub(super) fn references_equal(&self, pools: &SemanticPools, scope: SchemeScopeId, left: TypeRef, right: TypeRef) -> Result<bool, IrVerifyError> {
        let scope = self.scope(scope)?;
        Ok(self.normalized(pools, scope, left, None, &mut Vec::new())? == self.normalized(pools, scope, right, None, &mut Vec::new())?)
    }
    pub(super) fn reference_equals_ground(&self, pools: &SemanticPools, scope: SchemeScopeId, reference: TypeRef, ground: crate::sema::types::Type) -> Result<bool, IrVerifyError> {
        Ok(self.normalized(pools, self.scope(scope)?, reference, None, &mut Vec::new())? == normalize_ground(ground))
    }
    pub(super) fn call_result_matches(&self, pools: &SemanticPools, scope: SchemeScopeId, instruction: u32, reference: TypeRef) -> Result<bool, IrVerifyError> {
        let call = self.call(instruction).ok_or_else(|| failure("symbolic source call lacks prepared evidence"))?;
        let expected = self.normalized(pools, self.scope(scope)?, reference, None, &mut Vec::new())?;
        let actual = match call.evidence {
            CallEvidence::Ground(id) => normalize_ground(pools.to_type(self.instance(id)?.result_type)?),
            CallEvidence::Forwarded(id) => {
                let plan = self.forwarding(id)?;
                if plan.caller != scope { return Err(failure("symbolic source call belongs to another scope")); }
                let substitutions = plan.substitutions.iter().map(|&reference| self.normalized(pools, self.scope(scope)?, reference, None, &mut Vec::new())).collect::<Result<Vec<_>, _>>()?;
                let target = self.scope(plan.callee)?;
                self.normalized(pools, target, target.result, Some(&substitutions), &mut Vec::new())?
            }
        };
        Ok(expected == actual)
    }
}
