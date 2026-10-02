use super::generic::{InstructionOwner, PatternApplicationId, PatternCaptureId, PatternSourceId, SchemeScopeId, TypeRef};
use super::{IrBlockId, IrVerifyError};
use crate::sema::check::{DeclarationIdentity, ExpressionIdentity, PatternCaptureIdentity, PatternIdentity, StatementIdentity};
use crate::sema::constants::LiteralConstant;
use crate::symbol::Name;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum PreparedPatternNominalKind { Tag, Error }

/// The declaring identity and ordered fields come from original registration.
/// Pattern expectations are checked against this separate retained receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedPatternNominalMember {
    pub identity: crate::sema::check::QualifiedNominalIdentity,
    pub kind: PreparedPatternNominalKind,
    pub family: Name,
    pub member: Name,
    pub tested: TypeRef,
    pub fields: Box<[(Option<Name>, TypeRef)]>,
    pub facets: Box<[Name]>,
}

impl PreparedPatternNominalMember {
    pub fn retained_bytes(&self) -> usize {
        self.fields.len() * std::mem::size_of::<(Option<Name>, TypeRef)>()
            + self.facets.len() * std::mem::size_of::<Name>()
    }
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) enum PreparedPatternShape {
    Wildcard,
    Binding,
    Literal(LiteralConstant),
    Group,
    Alias { name: Name },
    List { elements: u32, has_rest: bool },
    Record { fields: Box<[Name]> },
    Alternation,
    Type,
    Facet,
    Constructor,
    TestName,
    Tuple { fields: Box<[TypeRef]> },
    ErrorVariant { fields: Box<[Name]> },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum PreparedPatternDecision { Structural, Alternation, Binding, Type, Result { success: bool, payload: Option<TypeRef> },
    TagConstructor { identity: crate::sema::check::QualifiedNominalIdentity, family: Name, member: Name },
    TagFields,
    ErrorVariant { identity: crate::sema::check::QualifiedNominalIdentity, family: Name, member: Name },
    Facet { facet: Name },
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PatternSourceCapture {
    pub identity: PatternCaptureIdentity,
    pub expected: TypeRef,
    pub branches: Box<[PatternCaptureIdentity]>,
    pub slot: u32,
}

/// Source expectations are copied from checked identities before encoding.
/// Their slot allocation is independent of the slot words that verification
/// subsequently reads from executable patterns and identifier instructions.
#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedPatternSource {
    pub origin: PatternIdentity,
    pub caller: Option<DeclarationIdentity>,
    pub scope: Option<SchemeScopeId>,
    pub input: TypeRef,
    pub tested: Option<TypeRef>,
    pub input_nominal: Option<crate::sema::check::QualifiedNominalIdentity>,
    pub tested_nominal: Option<crate::sema::check::QualifiedNominalIdentity>,
    pub shape: PreparedPatternShape,
    pub decision: PreparedPatternDecision,
    pub children: Box<[PatternSourceId]>,
    pub captures: Box<[PatternSourceCapture]>,
}

impl PreparedPatternSource {
    pub fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        self.children.len() * size_of::<PatternSourceId>()
            + self.captures.len() * size_of::<PatternSourceCapture>()
            + self.captures.iter().map(|capture| capture.branches.len() * size_of::<PatternCaptureIdentity>()).sum::<usize>()
            + match &self.shape {
                PreparedPatternShape::Record { fields } | PreparedPatternShape::ErrorVariant { fields } => fields.len() * size_of::<Name>(),
                PreparedPatternShape::Tuple { fields } => fields.len() * size_of::<TypeRef>(),
                PreparedPatternShape::Literal(LiteralConstant::Str(value) | LiteralConstant::Path(value)) => value.len(),
                PreparedPatternShape::Literal(LiteralConstant::Bytes(value)) => value.len(),
                _ => 0,
            }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum PatternArmBody { Expression(u32), Statements(IrBlockId) }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum PreparedPatternAdmission {
    MatchArm,
    Conditional { control: u32, control_origin: super::generic::OperationSourceOrigin,
        condition_origin: ExpressionIdentity, branch: u32, body: PatternArmBody },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedPatternResultSource {
    pub origin: ExpressionIdentity,
    pub expected: TypeRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedPatternResultBody {
    pub instruction: u32,
    pub source: PreparedPatternResultSource,
    pub condition: Option<(u32, PreparedPatternResultSource)>,
    pub terminal: Option<(u32, u32, PreparedPatternResultSource, StatementIdentity)>,
}

/// Branch roots belong to the original value expression, independently of the
/// Bool matcher that admits each capture into its branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedPatternConditionalResult {
    pub control: u32,
    pub owner: InstructionOwner,
    pub scope: Option<SchemeScopeId>,
    pub source: PreparedPatternResultSource,
    pub branches: Box<[PreparedPatternResultBody]>,
    pub fallback: PreparedPatternResultBody,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedPatternApplication {
    pub source: PatternSourceId,
    pub owner: InstructionOwner,
    pub matcher: u32,
    pub subject: u32,
    pub subject_origin: ExpressionIdentity,
    pub pattern: u32,
    pub arm: u32,
    pub guard: Option<u32>,
    pub body: PatternArmBody,
    pub admission: PreparedPatternAdmission,
    pub result: Option<Box<PreparedPatternConditionalResult>>,
}

impl PreparedPatternApplication {
    pub fn retained_bytes(&self) -> usize {
        self.result.as_ref().map_or(0, |result| std::mem::size_of::<PreparedPatternConditionalResult>()
            + result.branches.len() * std::mem::size_of::<PreparedPatternResultBody>())
    }
}

pub(super) fn pattern_result_relation(
    store: &super::generic::GenericEvidenceStore, pools: &super::semantic::SemanticPools,
    scope: Option<SchemeScopeId>, actual: TypeRef, expected: TypeRef,
) -> Result<(), IrVerifyError> {
    if store.normalized_reference(pools, scope, actual)? == store.normalized_reference(pools, scope, expected)? { return Ok(()); }
    if let (TypeRef::Ground(actual), TypeRef::Ground(expected)) = (actual, expected) {
        let actual = pools.to_type(actual)?;
        let expected = pools.to_type(expected)?;
        if !actual.any_flows_to_concrete(&expected) && actual.matches_expected(&expected) { return Ok(()); }
    }
    Err(IrVerifyError::new("conditional branch lacks its original result compatibility"))
}

#[derive(Clone, Debug)]
pub(in crate::runtime::eval) struct PreparedPatternCapture {
    pub application: PatternApplicationId,
    pub identity: PatternCaptureIdentity,
    pub expected: TypeRef,
    pub slot: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) struct PreparedPatternUse {
    pub capture: PatternCaptureId,
    pub origin: SourceUseIdentity,
    pub owner: InstructionOwner,
    pub instruction: u32,
    pub slot: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::runtime::eval) enum SourceUseIdentity {
    Expression(ExpressionIdentity),
    Statement(StatementIdentity),
}

impl SourceUseIdentity {
    fn qualification(self) -> (crate::source::SourceId, Option<Name>) {
        match self { Self::Expression(identity) => (identity.source, identity.namespace), Self::Statement(identity) => (identity.source, identity.namespace) }
    }
    pub fn instruction_origin(self) -> super::generic::OperationSourceOrigin {
        match self { Self::Expression(identity) => super::generic::OperationSourceOrigin::Expression(identity), Self::Statement(identity) => super::generic::OperationSourceOrigin::Statement(identity) }
    }
}

pub(super) fn pattern_ground_type(
    pools: &super::semantic::SemanticPools, reference: TypeRef,
) -> Result<crate::sema::types::Type, IrVerifyError> {
    let TypeRef::Ground(ty) = reference else { return Err(IrVerifyError::new("scoped pattern capture evidence is not prepared")); };
    pools.to_type(ty)
}

pub(super) fn verify_pattern_nominal_member(
    pools: &super::semantic::SemanticPools, member: &PreparedPatternNominalMember,
) -> Result<(), IrVerifyError> {
    use crate::sema::check::{NominalDeclaration, QualifiedNominalIdentity};
    use crate::sema::types::Type;
    let failure = || IrVerifyError::new("original pattern nominal member receipt is invalid");
    let named = match member.identity {
        QualifiedNominalIdentity::Source { declaration, member: name, .. } => {
            if !matches!((member.kind, declaration), (PreparedPatternNominalKind::Tag, NominalDeclaration::Type(_)) | (PreparedPatternNominalKind::Error, NominalDeclaration::Error(_))) { return Err(failure()); }
            name
        }
        QualifiedNominalIdentity::Builtin { family, member: name } => {
            if family != member.family { return Err(failure()); } name
        }
    };
    if named != Some(member.member) { return Err(failure()); }
    let tested = pattern_ground_type(pools, member.tested)?;
    if !match (member.kind, tested) {
        (PreparedPatternNominalKind::Tag, Type::Tag(family)) => family == member.family,
        (PreparedPatternNominalKind::Error, Type::ErrorVariant { family, variant }) => family == member.family && variant == member.member,
        _ => false,
    } { return Err(failure()); }
    let mut labels = BTreeSet::new();
    for &(label, ty) in &member.fields {
        pattern_ground_type(pools, ty)?;
        match (member.kind, label) {
            (PreparedPatternNominalKind::Tag, None) => {},
            (PreparedPatternNominalKind::Error, Some(name)) if labels.insert(name) => {},
            _ => return Err(failure()),
        }
    }
    if (member.kind == PreparedPatternNominalKind::Tag && !member.facets.is_empty())
        || member.facets.iter().copied().collect::<BTreeSet<_>>().len() != member.facets.len() { return Err(failure()); }
    Ok(())
}

fn nominal_family(identity: crate::sema::check::QualifiedNominalIdentity) -> crate::sema::check::QualifiedNominalIdentity {
    use crate::sema::check::QualifiedNominalIdentity as Q;
    match identity {
        Q::Source { source, namespace, declaration, .. } => Q::Source { source, namespace, declaration, member: None },
        Q::Builtin { family, .. } => Q::Builtin { family, member: None },
    }
}

pub(super) fn visible_pattern_captures<'a>(
    store: &'a super::generic::GenericEvidenceStore, root: PatternSourceId,
) -> Result<Vec<&'a PatternSourceCapture>, IrVerifyError> {
    let mut captures = Vec::new();
    let mut pending = vec![(root, 0usize)];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some((id, depth)) = pending.pop() {
        if depth >= 512 || !seen.insert(id) { return Err(IrVerifyError::new("pattern source capture ancestry is cyclic or too deep")); }
        let source = store.pattern_source(id)?;
        captures.extend(source.captures.iter());
        if !matches!(source.shape, PreparedPatternShape::Alternation) {
            pending.extend(source.children.iter().rev().map(|&child| (child, depth + 1)));
        }
    }
    Ok(captures)
}

pub(super) fn verify_pattern_store(
    store: &super::generic::GenericEvidenceStore,
    pools: &super::semantic::SemanticPools,
    owners: &[Option<InstructionOwner>],
) -> Result<(), IrVerifyError> {
    use crate::sema::types::Type;
    use super::generic::NormalizedType as N;
    use super::generic::OperationSourceOrigin;
    let failure = || IrVerifyError::new("prepared pattern source relationship is invalid");
    let mut sources = BTreeMap::new();
    let mut parents = BTreeMap::new();
    let mut captures = BTreeMap::new();
    for (id, source) in store.pattern_sources() {
        store.pattern_source(id)?;
        if sources.insert(source.origin, id).is_some() { return Err(failure()); }
        if let Some(scope) = source.scope {
            let caller = source.caller.ok_or_else(failure)?;
            let target = store.scope(scope)?.owner;
            if store.scope_for_function(target) != Some(scope) || (caller.source, caller.namespace) != (source.origin.source, source.origin.namespace) { return Err(failure()); }
        }
        let view = |ty| store.normalized_reference(pools, source.scope, ty);
        let input = view(source.input)?;
        let tested = source.tested.map(view).transpose()?;
        for capture in &source.captures {
            if capture.identity.pattern != source.origin || captures.insert(capture.identity, capture).is_some() { return Err(failure()); }
            let expected = view(capture.expected)?;
            if capture.branches.is_empty() && expected != if source.decision == PreparedPatternDecision::Type { tested.clone().ok_or_else(failure)? } else { input.clone() } { return Err(failure()); }
        }
        let expected_children = match &source.shape {
            PreparedPatternShape::Wildcard | PreparedPatternShape::Binding | PreparedPatternShape::Literal(_) | PreparedPatternShape::Type | PreparedPatternShape::TestName | PreparedPatternShape::Facet => Some(0),
            PreparedPatternShape::Constructor => None,
            PreparedPatternShape::Group | PreparedPatternShape::Alias { .. } => Some(1),
            PreparedPatternShape::List { elements, has_rest } => Some(*elements as usize + usize::from(*has_rest)),
            PreparedPatternShape::Record { fields } | PreparedPatternShape::ErrorVariant { fields } => Some(fields.len()),
            PreparedPatternShape::Tuple { fields } => Some(fields.len()),
            PreparedPatternShape::Alternation => None,
        };
        if expected_children.is_some_and(|count| count != source.children.len()) { return Err(failure()); }
        if let PreparedPatternDecision::Result { success, payload } = source.decision {
            let N::Result(ok, error) = &input else { return Err(failure()); };
            let selected = if success { ok } else { error };
            if payload.map(view).transpose()?.as_ref() != Some(selected.as_ref()) { return Err(failure()); }
        }
        if let PreparedPatternDecision::Facet { facet } = source.decision {
            if tested != Some(N::Scalar(Type::ErrorFacet(facet)))
                || source.tested_nominal != Some(crate::sema::check::QualifiedNominalIdentity::Builtin { family: facet, member: None })
                || !matches!(input, N::Scalar(Type::Any | Type::Error | Type::ProcessError | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ErrorFacet(_)))
                || (matches!(input, N::Scalar(Type::ErrorFamily(_) | Type::ErrorVariant { .. })) && source.input_nominal.is_none()) { return Err(failure()); }
        }
        let nominal = match source.decision {
            PreparedPatternDecision::TagConstructor { identity, family, member } | PreparedPatternDecision::ErrorVariant { identity, family, member } => {
                let original = store.pattern_nominal(identity)?;
                let kind = if matches!(source.decision, PreparedPatternDecision::TagConstructor { .. }) { PreparedPatternNominalKind::Tag } else { PreparedPatternNominalKind::Error };
                if original.kind != kind || original.family != family || original.member != member { return Err(failure()); }
                if source.input_nominal.is_some_and(|input| nominal_family(input) != nominal_family(identity))
                    || source.tested_nominal.is_some_and(|tested| nominal_family(tested) != nominal_family(identity)) { return Err(failure()); }
                if kind == PreparedPatternNominalKind::Tag {
                    if tested.as_ref() != Some(&view(original.tested)?) || !matches!(&input, N::Scalar(Type::Any | Type::Tag(_))) { return Err(failure()); }
                    if matches!(&input, N::Scalar(Type::Tag(_))) && source.input_nominal.is_none() { return Err(failure()); }
                    if source.tested_nominal.is_none() { return Err(failure()); }
                    if matches!(&input, N::Scalar(Type::Tag(_))) && input != view(original.tested)? { return Err(failure()); }
                    if source.children.len() != usize::from(!original.fields.is_empty()) { return Err(failure()); }
                    if original.fields.len() > 1 {
                        let tuple = store.pattern_source(source.children[0])?;
                        let PreparedPatternShape::Tuple { fields } = &tuple.shape else { return Err(failure()); };
                        if fields.as_ref() != original.fields.iter().map(|(_, ty)| *ty).collect::<Vec<_>>().as_slice() { return Err(failure()); }
                    }
                } else {
                    if !matches!(input, N::Scalar(Type::Any | Type::Error | Type::ProcessError | Type::ErrorFamily(_) | Type::ErrorVariant { .. })) { return Err(failure()); }
                    if matches!(&input, N::Scalar(Type::ErrorFamily(_) | Type::ErrorVariant { .. })) && source.input_nominal.is_none() { return Err(failure()); }
                    if source.tested_nominal != Some(identity) { return Err(failure()); }
                    if matches!(&input, N::Scalar(Type::ErrorVariant { .. })) && source.input_nominal != Some(identity) { return Err(failure()); }
                }
                Some(original)
            }
            _ => None,
        };
        for (ordinal, &child) in source.children.iter().enumerate() {
            let child = store.pattern_source(child)?;
            if (child.origin.source, child.origin.namespace, child.caller, child.scope) != (source.origin.source, source.origin.namespace, source.caller, source.scope)
                || parents.insert(child.origin, source.origin).is_some() { return Err(failure()); }
            let child_input = view(child.input)?;
            let result_payload = match source.decision {
                PreparedPatternDecision::Result { payload: Some(ty), .. } => Some(view(ty)?),
                _ => None,
            };
            let nominal_field = match (&source.shape, nominal) {
                (PreparedPatternShape::ErrorVariant { fields }, Some(original)) => Some(view(original.fields.iter().find(|(name, _)| *name == Some(fields[ordinal])).ok_or_else(failure)?.1)?),
                (_, Some(original)) if original.kind == PreparedPatternNominalKind::Tag && original.fields.len() == 1 => Some(view(original.fields[0].1)?),
                (PreparedPatternShape::Tuple { fields }, _) => Some(view(fields[ordinal])?),
                _ => None,
            };
            let dynamic_rest = matches!(&input, N::Scalar(Type::Any)).then(|| N::List(Box::new(input.clone())));
            let expected = match &source.shape {
                _ if nominal_field.is_some() => nominal_field.as_ref().unwrap(),
                PreparedPatternShape::Constructor if result_payload.is_some() => result_payload.as_ref().unwrap(),
                PreparedPatternShape::List { elements, .. } => {
                    match &input {
                        N::List(item) => if ordinal < *elements as usize { item.as_ref() } else { &input },
                        N::Scalar(Type::Any) => if ordinal < *elements as usize { &input } else { dynamic_rest.as_ref().unwrap() },
                        _ => return Err(failure()),
                    }
                }
                PreparedPatternShape::Record { fields } => {
                    let N::Record(types, _) = &input else { return Err(failure()); };
                    types.get(&fields[ordinal]).ok_or_else(failure)?
                }
                _ => &input,
            };
            if &child_input != expected { return Err(failure()); }
        }
        match (&source.shape, source.decision) {
            (PreparedPatternShape::Binding, PreparedPatternDecision::Binding) if source.captures.len() == 1 => {},
            (PreparedPatternShape::Alias { name }, PreparedPatternDecision::Structural) if source.captures.len() == 1 && source.captures[0].identity.name == *name => {},
            (PreparedPatternShape::Alternation, PreparedPatternDecision::Alternation) if source.children.len() >= 2 => {},
            (PreparedPatternShape::Constructor | PreparedPatternShape::TestName | PreparedPatternShape::Binding | PreparedPatternShape::ErrorVariant { .. }, PreparedPatternDecision::TagConstructor { .. }) if source.captures.is_empty() => {},
            (PreparedPatternShape::ErrorVariant { .. } | PreparedPatternShape::TestName, PreparedPatternDecision::ErrorVariant { .. }) if source.captures.is_empty() => {},
            (PreparedPatternShape::Tuple { .. }, PreparedPatternDecision::TagFields) if source.captures.is_empty() => {},
            (PreparedPatternShape::Constructor | PreparedPatternShape::TestName, PreparedPatternDecision::Result { .. }) if source.children.len() <= 1 && source.captures.is_empty() => {},
            (PreparedPatternShape::Type, PreparedPatternDecision::Type) if source.tested.is_some() && source.captures.len() <= 1 => {},
            (PreparedPatternShape::Facet | PreparedPatternShape::TestName, PreparedPatternDecision::Facet { .. }) if source.captures.is_empty() => {},
            (PreparedPatternShape::Wildcard | PreparedPatternShape::Literal(_) | PreparedPatternShape::Group | PreparedPatternShape::List { .. } | PreparedPatternShape::Record { .. }, PreparedPatternDecision::Structural) if source.captures.is_empty() => {},
            _ => return Err(failure()),
        }
    }
    let descendant = |root: PatternIdentity, mut child: PatternIdentity| -> Result<bool, IrVerifyError> {
        for _ in 0..=512 {
            if root == child { return Ok(true); }
            match parents.get(&child) { Some(parent) => child = *parent, None => return Ok(false) }
        }
        Err(failure())
    };
    for (_, source) in store.pattern_sources() {
        for capture in &source.captures {
            if !matches!(source.shape, PreparedPatternShape::Alternation) {
                if !capture.branches.is_empty() { return Err(failure()); }
                continue;
            }
            if capture.branches.len() != source.children.len() { return Err(failure()); }
            let expected = store.normalized_reference(pools, source.scope, capture.expected)?;
            for (&branch, &child) in capture.branches.iter().zip(source.children.iter()) {
                let child = store.pattern_source(child)?;
                let original = captures.get(&branch).ok_or_else(failure)?;
                if branch.name != capture.identity.name || !descendant(child.origin, branch.pattern)?
                    || original.slot != capture.slot || store.normalized_reference(pools, source.scope, original.expected)? != expected { return Err(failure()); }
            }
        }
    }
    let mut applications = BTreeSet::new();
    let mut reached = rustc_hash::FxHashSet::default();
    let mut visible = rustc_hash::FxHashMap::default();
    for (id, _) in store.pattern_applications() {
        let application = store.pattern_application(id)?;
        let source = store.pattern_source(application.source)?;
        let owner_matches = match source.caller {
            Some(_) if source.scope.is_some() => application.owner == InstructionOwner::Function(store.scope(source.scope.unwrap())?.owner),
            Some(caller) => application.owner == InstructionOwner::Function(store.checked_function(caller)?.target),
            None => matches!(application.owner, InstructionOwner::Driver(_)),
        };
        if !owner_matches
            || (application.subject_origin.source, application.subject_origin.namespace) != (source.origin.source, source.origin.namespace)
            || store.registered_pattern_origin(application.pattern) != Some((source.origin, application.owner))
            || !applications.insert((application.matcher, application.arm))
            || owners.get(application.matcher as usize) != Some(&Some(application.owner))
            || owners.get(application.subject as usize) != Some(&Some(application.owner))
            || store.registered_instruction_origin(application.subject, false) != Some((OperationSourceOrigin::Expression(application.subject_origin), application.owner)) { return Err(failure()); }
        if let PreparedPatternAdmission::Conditional { control, control_origin, condition_origin, .. } = application.admission {
            let qualification = match control_origin {
                OperationSourceOrigin::Expression(identity) => (identity.source, identity.namespace),
                OperationSourceOrigin::Statement(identity) => (identity.source, identity.namespace),
                _ => return Err(failure()),
            };
            if qualification != (source.origin.source, source.origin.namespace)
                || (condition_origin.source, condition_origin.namespace) != qualification
                || owners.get(control as usize) != Some(&Some(application.owner))
                || store.registered_instruction_origin(control, false) != Some((control_origin, application.owner))
                || store.registered_instruction_origin(application.matcher, false) != Some((OperationSourceOrigin::Expression(condition_origin), application.owner)) { return Err(failure()); }
        }
        if let Some(result) = &application.result {
            let PreparedPatternAdmission::Conditional { control, control_origin: OperationSourceOrigin::Expression(origin), .. } = application.admission else { return Err(failure()); };
            if result.control != control || result.owner != application.owner || result.scope != source.scope
                || result.source.origin != origin || result.branches.is_empty() || result.fallback.condition.is_some() { return Err(failure()); }
            store.normalized_reference(pools, result.scope, result.source.expected)?;
            for (ordinal, body) in result.branches.iter().chain(std::iter::once(&result.fallback)).enumerate() {
                if ordinal < result.branches.len() && body.condition.is_none() { return Err(failure()); }
                let check = |instruction, original: &PreparedPatternResultSource| -> Result<(), IrVerifyError> {
                    if owners.get(instruction as usize) != Some(&Some(result.owner))
                        || (original.origin.source, original.origin.namespace) != (origin.source, origin.namespace)
                        || store.registered_instruction_origin(instruction, false) != Some((OperationSourceOrigin::Expression(original.origin), result.owner)) { return Err(failure()); }
                    store.normalized_reference(pools, result.scope, original.expected)?;
                    Ok(())
                };
                check(body.instruction, &body.source)?;
                pattern_result_relation(store, pools, result.scope, body.source.expected, result.source.expected)?;
                if let Some((condition, expected)) = &body.condition {
                    check(*condition, expected)?;
                    if store.normalized_reference(pools, result.scope, expected.expected)? != N::Scalar(Type::Bool) { return Err(failure()); }
                }
                if let Some((statement, value, original, identity)) = &body.terminal {
                    check(*value, original)?;
                    if owners.get(*statement as usize) != Some(&Some(result.owner))
                        || (identity.source, identity.namespace) != (origin.source, origin.namespace)
                        || store.registered_instruction_origin(*statement, false) != Some((OperationSourceOrigin::Statement(*identity), result.owner)) { return Err(failure()); }
                    pattern_result_relation(store, pools, result.scope, original.expected, body.source.expected)?;
                }
            }
        }
        let mut pending = vec![application.source];
        while let Some(source) = pending.pop() { if reached.insert(source) { pending.extend(store.pattern_source(source)?.children.iter().copied()); } }
        let captures = visible_pattern_captures(store, application.source)?;
        let mut names = BTreeSet::new();
        if captures.iter().any(|capture| !names.insert(capture.identity.name)) { return Err(failure()); }
        visible.insert(id, captures);
    }
    if reached.len() != sources.len() { return Err(failure()); }
    let mut published_captures = rustc_hash::FxHashSet::default();
    for (id, capture) in store.pattern_captures() {
        let application = store.pattern_application(capture.application)?;
        let source = visible.get(&capture.application).ok_or_else(failure)?.iter().find(|source| source.identity == capture.identity).ok_or_else(failure)?;
        if source.expected != capture.expected || source.slot != capture.slot || !published_captures.insert((capture.application, capture.identity)) { return Err(failure()); }
        store.pattern_capture(id)?;
        let _ = application;
    }
    if published_captures.len() != visible.values().map(Vec::len).sum::<usize>() { return Err(failure()); }
    let mut previous = None;
    for use_ in store.pattern_uses() {
        let capture = store.pattern_capture(use_.capture)?;
        let application = store.pattern_application(capture.application)?;
        if previous.is_some_and(|instruction| instruction >= use_.instruction) || use_.slot != capture.slot || use_.owner != application.owner
            || owners.get(use_.instruction as usize) != Some(&Some(use_.owner))
            || use_.origin.qualification() != (capture.identity.pattern.source, capture.identity.pattern.namespace)
            || store.registered_instruction_origin(use_.instruction, false) != Some((use_.origin.instruction_origin(), use_.owner)) { return Err(failure()); }
        previous = Some(use_.instruction);
    }
    Ok(())
}

/// Parent edges come from the executable's existing structural verifier.
/// Pattern visibility uses ancestry of its real guard or body, rather than
/// instruction numbering or a source span that may overlap another arm.
pub(in crate::runtime::eval) struct PatternTreeBuilder {
    parents: Vec<Option<u32>>,
    seen: Vec<bool>,
    active: Vec<u32>,
    unbalanced: bool,
}

pub(in crate::runtime::eval) struct PatternTree {
    parents: Vec<Option<u32>>,
    seen: Vec<bool>,
}

impl PatternTreeBuilder {
    pub fn new(instruction_count: usize) -> Self {
        Self { parents: vec![None; instruction_count], seen: vec![false; instruction_count], active: Vec::new(), unbalanced: false }
    }
    pub fn enter(&mut self, instruction: u32) -> Result<(), IrVerifyError> {
        if self.active.len() >= 512 { return Err(IrVerifyError::new("pattern visibility ancestry is too deep")); }
        let index = instruction as usize;
        let seen = self.seen.get_mut(index).ok_or_else(|| IrVerifyError::new("pattern visibility instruction is out of bounds"))?;
        if *seen { return Err(IrVerifyError::new("pattern visibility instruction has multiple structural parents")); }
        *seen = true;
        self.parents[index] = self.active.last().copied();
        self.active.push(instruction);
        Ok(())
    }
    pub fn exit(&mut self, instruction: u32) {
        if self.active.pop() != Some(instruction) { self.unbalanced = true; }
    }
    pub fn finish(self) -> Result<PatternTree, IrVerifyError> {
        if self.unbalanced || !self.active.is_empty() { return Err(IrVerifyError::new("pattern visibility traversal is unbalanced")); }
        Ok(PatternTree { parents: self.parents, seen: self.seen })
    }
}

impl PatternTree {
    pub fn parent(&self, instruction: u32) -> Result<Option<u32>, IrVerifyError> {
        if self.seen.get(instruction as usize) != Some(&true) { return Err(IrVerifyError::new("lexical instruction lacks structural ownership")); }
        Ok(self.parents[instruction as usize])
    }
    pub fn is_descendant(&self, root: u32, mut instruction: u32) -> Result<bool, IrVerifyError> {
        if self.seen.get(root as usize) != Some(&true) || self.seen.get(instruction as usize) != Some(&true) {
            return Err(IrVerifyError::new("pattern visibility instruction lacks structural ownership"));
        }
        for _ in 0..=512 {
            if instruction == root { return Ok(true); }
            match self.parents[instruction as usize] { Some(parent) => instruction = parent, None => return Ok(false) }
        }
        Err(IrVerifyError::new("pattern visibility ancestry is too deep"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_visibility_uses_actual_nested_arm_ancestry() {
        let mut tree = PatternTreeBuilder::new(8);
        tree.enter(7).unwrap();
        tree.enter(3).unwrap(); tree.enter(1).unwrap(); tree.exit(1); tree.exit(3);
        tree.enter(6).unwrap(); tree.enter(2).unwrap(); tree.exit(2); tree.exit(6);
        tree.exit(7);
        let tree = tree.finish().unwrap();
        assert!(tree.is_descendant(3, 1).unwrap());
        assert!(!tree.is_descendant(3, 2).unwrap());
        assert!(!tree.is_descendant(3, 6).unwrap());
        assert!(tree.is_descendant(7, 1).unwrap());
        assert!(tree.is_descendant(3, 3).unwrap());
        assert!(tree.is_descendant(3, 0).is_err());
    }
}
