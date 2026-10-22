use super::{EffectDeclarationId, Name, Type};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(super) enum StreamProjection {
    Success,
    Field(Name),
    Element,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct ProjectionPath(Vec<StreamProjection>);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StreamOriginBoundaryError;

impl ProjectionPath {
    fn new(path: Vec<StreamProjection>) -> Result<Self, StreamOriginBoundaryError> {
        if path.len() > crate::sema::constraints::MAX_TYPE_DEPTH { return Err(StreamOriginBoundaryError); }
        Ok(Self(path))
    }

    fn prefix(&self, step: StreamProjection) -> Result<Self, StreamOriginBoundaryError> {
        let mut path = Vec::with_capacity(self.0.len() + 1);
        path.push(step);
        path.extend_from_slice(&self.0);
        Self::new(path)
    }

    fn append(&self, steps: &[StreamProjection]) -> Result<Self, StreamOriginBoundaryError> {
        let mut path = self.0.clone();
        path.extend_from_slice(steps);
        Self::new(path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum StreamOrigin {
    Error,
    Opaque,
    Pending(EffectDeclarationId),
    Callable(EffectDeclarationId),
    OriginalApplication { reference: crate::sema::constraints::CallableApplicationReference, projection: ProjectionPath },
    BinderApplication { owner: EffectDeclarationId, ordinal: u32, projection: ProjectionPath },
    DeferredApplication { owner: EffectDeclarationId, target: EffectDeclarationId, span: Span, ordinal: u32, projection: ProjectionPath },
    Formal { owner: EffectDeclarationId, parameter: usize, projection: ProjectionPath },
    OpaqueFormal { owner: EffectDeclarationId, parameter: usize, projection: ProjectionPath },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StreamOrigins(BTreeSet<(ProjectionPath, StreamOrigin)>);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ProjectionDomain {
    paths: BTreeSet<ProjectionPath>,
    opaque: BTreeSet<ProjectionPath>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StreamReturnSummary {
    returned: StreamOrigins,
    defaults: BTreeMap<usize, StreamOrigins>,
    output: ProjectionDomain,
    parameters: Vec<ProjectionDomain>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct StreamLoopId {
    owner: Option<EffectDeclarationId>,
    namespace: Option<Name>,
    block: crate::syntax::arena::BlockId,
    body: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StreamLoopKind { For(crate::syntax::arena::ExprId), While(crate::syntax::arena::ExprId), Loop }

#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub(super) enum StreamFlow {
    #[default]
    Live,
    Dead,
}

#[derive(Clone, Copy)]
pub(super) enum StreamLoopTransfer { Backedge, Break }

#[derive(Clone)]
pub(super) struct StreamLoopFrame {
    identity: StreamLoopId,
    kind: StreamLoopKind,
    enclosing_flow: StreamFlow,
    body_flow: StreamFlow,
    types: BTreeMap<super::BindingId, Type>,
    header: BTreeMap<super::BindingId, StreamOrigins>,
    breaks: BTreeMap<super::BindingId, StreamOrigins>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct StreamOriginSummaries {
    declarations: BTreeMap<EffectDeclarationId, StreamReturnSummary>,
    loops: BTreeMap<StreamLoopId, BTreeMap<super::BindingId, StreamOrigins>>,
}

impl StreamOriginSummaries {
    pub(super) fn new() -> Self { Self::default() }

    pub(super) fn extend(&mut self, other: Self) {
        self.declarations.extend(other.declarations);
        self.loops.extend(other.loops);
    }

    pub(super) fn append(&mut self, other: &mut Self) {
        self.declarations.append(&mut other.declarations);
        self.loops.append(&mut other.loops);
    }

    pub(super) fn reject_declarations(&mut self, rejected: &BTreeSet<EffectDeclarationId>) {
        self.declarations.retain(|owner, _| !rejected.contains(owner));
        self.loops.retain(|identity, _| identity.owner.is_none_or(|owner| !rejected.contains(&owner)));
    }
}

impl ProjectionDomain {
    fn from_type(ty: &Type) -> Result<Self, StreamOriginBoundaryError> {
        let mut output = Self::default();
        let mut pending = vec![(ty, ProjectionPath::default(), 0usize)];
        while let Some((ty, path, depth)) = pending.pop() {
            if depth > crate::sema::constraints::MAX_TYPE_DEPTH { return Err(StreamOriginBoundaryError); }
            output.paths.insert(path.clone());
            match ty {
                Type::Result(ok, _) => pending.push((ok, path.append(&[StreamProjection::Success])?, depth + 1)),
                Type::List(item) | Type::Set(item) | Type::Stream(item) => pending.push((item, path.append(&[StreamProjection::Element])?, depth + 1)),
                Type::Map(_, value) => pending.push((value, path.append(&[StreamProjection::Element])?, depth + 1)),
                Type::Record(fields) => for (&name, value) in fields.iter() {
                    pending.push((value, path.append(&[StreamProjection::Field(name)])?, depth + 1));
                },
                Type::Optional(value) => pending.push((value, path, depth + 1)),
                Type::Validated(value) => pending.push((value.base(), path, depth + 1)),
                Type::Any | Type::Unknown | Type::Invalid | Type::Inference(_) | Type::BuiltinParameter(_)
                    | Type::Tag(_) | Type::ErasedRecord | Type::DynamicModule => { output.opaque.insert(path); }
                _ => {}
            }
        }
        Ok(output)
    }

    fn boundary(&self, path: &ProjectionPath) -> Result<Option<ProjectionPath>, StreamOriginBoundaryError> {
        if self.paths.contains(path) { return Ok(None); }
        if let Some(prefix) = self.opaque.iter().find(|prefix| path.0.starts_with(&prefix.0)) { return Ok(Some(prefix.clone())); }
        Err(StreamOriginBoundaryError)
    }
}

impl StreamOrigins {
    pub(super) fn unproven(ty: &Type) -> Self {
        if ty.may_hold_resource() { Self::opaque() } else { Self::default() }
    }

    pub(super) fn formal(owner: EffectDeclarationId, parameter: usize) -> Self {
        Self([(ProjectionPath::default(), StreamOrigin::Formal { owner, parameter, projection: ProjectionPath::default() })].into_iter().collect())
    }

    pub(super) fn application(reference: crate::sema::constraints::CallableApplicationReference) -> Self {
        Self([(ProjectionPath::default(), StreamOrigin::OriginalApplication { reference, projection: ProjectionPath::default() })].into_iter().collect())
    }

    pub(super) fn return_applications(&self) -> BTreeSet<Option<crate::sema::constraints::CallableApplicationReference>> {
        let mut applications = self.0.iter().map(|(path, origin)| match origin {
            StreamOrigin::OriginalApplication { reference, projection } if path.0.is_empty() && projection.0.is_empty() => Some(*reference),
            _ => None,
        }).collect::<BTreeSet<_>>();
        if applications.is_empty() { applications.insert(None); }
        applications
    }

    fn specialize_applications(&self, source: EffectDeclarationId, instances: Option<&[crate::sema::constraints::CallableApplicationId]>, constraints: &crate::sema::constraints::TypeConstraints, pending: Option<&super::PendingFunctionSig>, deferred_call: Option<(EffectDeclarationId, Span)>) -> Result<Self, StreamOriginBoundaryError> {
        let application = |ordinal: u32, projection: &ProjectionPath| {
            if let Some(instances) = instances {
                let id = *instances.get(ordinal as usize).ok_or(StreamOriginBoundaryError)?;
                return Ok(StreamOrigin::OriginalApplication { reference: constraints.callable_application_reference(id).map_err(|_| StreamOriginBoundaryError)?, projection: projection.clone() });
            }
            let (owner, span) = deferred_call.ok_or(StreamOriginBoundaryError)?;
            Ok(StreamOrigin::DeferredApplication { owner, target: source, span, ordinal, projection: projection.clone() })
        };
        let mut output = Self::default();
        for (path, origin) in &self.0 {
            let origin = match origin {
                StreamOrigin::OriginalApplication { reference, projection } => {
                    let pending = pending.ok_or(StreamOriginBoundaryError)?;
                    let ordinal = constraints.generalized_application_ordinal(*reference, source, pending.generalized().ok_or(StreamOriginBoundaryError)?, pending.generalized_scope().ok_or(StreamOriginBoundaryError)?)
                        .map_err(|_| StreamOriginBoundaryError)?.ok_or(StreamOriginBoundaryError)?;
                    application(ordinal, projection)?
                }
                StreamOrigin::BinderApplication { owner, ordinal, projection } => {
                    if *owner != source { return Err(StreamOriginBoundaryError); }
                    application(*ordinal, projection)?
                }
                origin => origin.clone(),
            };
            output.0.insert((path.clone(), origin));
        }
        Ok(output)
    }

    fn normalize_applications(&self, owner: EffectDeclarationId, constraints: &crate::sema::constraints::TypeConstraints, pending: Option<&super::PendingFunctionSig>) -> Result<Self, StreamOriginBoundaryError> {
        let mut output = Self::default();
        for (path, origin) in &self.0 {
            let origin = match origin {
                StreamOrigin::OriginalApplication { reference, projection } => {
                    let pending = pending.ok_or(StreamOriginBoundaryError)?;
                    let translated = constraints.generalized_application_ordinal(*reference, owner, pending.generalized().ok_or(StreamOriginBoundaryError)?, pending.generalized_scope().ok_or(StreamOriginBoundaryError)?);
                    let ordinal = translated.map_err(|_| StreamOriginBoundaryError)?.ok_or(StreamOriginBoundaryError)?;
                    StreamOrigin::BinderApplication { owner, ordinal, projection: projection.clone() }
                }
                StreamOrigin::BinderApplication { owner: actual_owner, ordinal, .. } => {
                    if *actual_owner != owner || pending.and_then(|pending| pending.generalized()).and_then(|callable| callable.requirements().applications.get(*ordinal as usize)).is_none() {
                        return Err(StreamOriginBoundaryError);
                    }
                    origin.clone()
                }
                StreamOrigin::DeferredApplication { .. } => return Err(StreamOriginBoundaryError),
                origin => origin.clone(),
            };
            output.0.insert((path.clone(), origin));
        }
        Ok(output)
    }

    fn resolve_applications(&self, owner: EffectDeclarationId, constraints: &crate::sema::constraints::TypeConstraints,
        calls: &BTreeMap<Span, super::PendingCallInstantiation>, edges: &BTreeMap<Span, (EffectDeclarationId, super::FunctionDeclarationId)>,
    ) -> Result<Self, StreamOriginBoundaryError> {
        let mut output = Self::default();
        for (path, origin) in &self.0 {
            let origin = match origin {
                StreamOrigin::DeferredApplication { owner: actual_owner, target, span, ordinal, projection } => {
                    if *actual_owner != owner || edges.get(span) != Some(&(owner, super::FunctionDeclarationId::new(*target))) {
                        return Err(StreamOriginBoundaryError);
                    }
                    let (callable, instance) = calls.get(span).ok_or(StreamOriginBoundaryError)?;
                    if instance.applications.len() != callable.requirements().applications.len() { return Err(StreamOriginBoundaryError); }
                    let id = *instance.applications.get(*ordinal as usize).ok_or(StreamOriginBoundaryError)?;
                    StreamOrigin::OriginalApplication { reference: constraints.callable_application_reference(id).map_err(|_| StreamOriginBoundaryError)?, projection: projection.clone() }
                }
                origin => origin.clone(),
            };
            output.0.insert((path.clone(), origin));
        }
        Ok(output)
    }

    fn has_unresolved_applications(&self) -> bool {
        self.0.iter().any(|(_, origin)| matches!(origin, StreamOrigin::OriginalApplication { .. } | StreamOrigin::DeferredApplication { .. }))
    }

    pub(super) fn callable(target: EffectDeclarationId) -> Self {
        Self([(ProjectionPath::default(), StreamOrigin::Callable(target))].into_iter().collect())
    }

    pub(super) fn pending(target: EffectDeclarationId) -> Self {
        Self([(ProjectionPath::default(), StreamOrigin::Pending(target))].into_iter().collect())
    }

    fn has_unknown_callable(&self) -> bool {
        self.0.iter().any(|(path, origin)| path.0.is_empty() && !matches!(origin, StreamOrigin::Callable(_)))
    }

    fn replace_projection(&self, root: &Type, path: &[StreamProjection], value: &Self) -> Result<Self, StreamOriginBoundaryError> {
        ProjectionPath::new(path.to_vec())?;
        let Some((head, remaining)) = path.split_first() else { return Ok(value.clone()) };
        if let StreamProjection::Field(selected) = head
            && let Type::Record(fields) = root.unvalidated()
        {
            let mut output = Self::default();
            for (&name, ty) in fields.iter() {
                let field = StreamProjection::Field(name);
                let mut origin = self.project(std::slice::from_ref(&field))?;
                if name == *selected { origin = origin.replace_projection(ty, remaining, value)?; }
                output.join(&origin.prefix(field)?);
            }
            return Ok(output);
        }
        let mut output = self.clone();
        let mut value = value.clone();
        for step in path.iter().rev() { value = value.prefix(step.clone())?; }
        output.join(&value);
        Ok(output)
    }

    pub(super) fn targets(&self) -> impl Iterator<Item = EffectDeclarationId> + '_ {
        self.0.iter().filter_map(|(path, origin)| match origin {
            StreamOrigin::Callable(target) if path.0.is_empty() => Some(*target),
            StreamOrigin::Callable(_) | StreamOrigin::Error | StreamOrigin::Opaque | StreamOrigin::Pending(_) | StreamOrigin::OriginalApplication { .. } | StreamOrigin::BinderApplication { .. } | StreamOrigin::DeferredApplication { .. }
                | StreamOrigin::Formal { .. } | StreamOrigin::OpaqueFormal { .. } => None,
        })
    }

    pub(super) fn error() -> Self {
        Self([(ProjectionPath::default(), StreamOrigin::Error)].into_iter().collect())
    }

    pub(super) fn opaque() -> Self {
        Self([(ProjectionPath::default(), StreamOrigin::Opaque)].into_iter().collect())
    }

    pub(super) fn join(&mut self, other: &Self) -> bool {
        let previous = self.0.len();
        self.0.extend(other.0.iter().cloned());
        self.0.len() != previous
    }

    pub(super) fn prefix(&self, step: StreamProjection) -> Result<Self, StreamOriginBoundaryError> {
        self.0.iter().map(|(path, source)| Ok((path.prefix(step.clone())?, source.clone()))).collect::<Result<BTreeSet<_>, _>>().map(Self)
    }

    pub(super) fn project(&self, requested: &[StreamProjection]) -> Result<Self, StreamOriginBoundaryError> {
        let requested = ProjectionPath::new(requested.to_vec())?;
        let mut output = Self::default();
        for (path, source) in &self.0 {
            if let Some(remaining) = path.0.strip_prefix(requested.0.as_slice()) {
                output.0.insert((ProjectionPath::new(remaining.to_vec())?, source.clone()));
            } else if let Some(remaining) = requested.0.strip_prefix(path.0.as_slice()) {
                let projected = match source {
                    StreamOrigin::Formal { owner, parameter, projection } => StreamOrigin::Formal { owner: *owner, parameter: *parameter, projection: projection.append(remaining)? },
                    StreamOrigin::OpaqueFormal { owner, parameter, projection } => StreamOrigin::OpaqueFormal { owner: *owner, parameter: *parameter, projection: projection.clone() },
                    StreamOrigin::OriginalApplication { reference, projection } => StreamOrigin::OriginalApplication { reference: *reference, projection: projection.append(remaining)? },
                    StreamOrigin::BinderApplication { owner, ordinal, projection } => StreamOrigin::BinderApplication { owner: *owner, ordinal: *ordinal, projection: projection.append(remaining)? },
                    StreamOrigin::DeferredApplication { owner, target, span, ordinal, projection } => StreamOrigin::DeferredApplication { owner: *owner, target: *target, span: *span, ordinal: *ordinal, projection: projection.append(remaining)? },
                    StreamOrigin::Opaque => StreamOrigin::Opaque,
                    StreamOrigin::Pending(target) => StreamOrigin::Pending(*target),
                    StreamOrigin::Error | StreamOrigin::Callable(_) => continue,
                };
                output.0.insert((ProjectionPath::default(), projected));
            }
        }
        Ok(output)
    }

    pub(super) fn substitute(&self, arguments: &BTreeMap<usize, Self>) -> Result<Self, StreamOriginBoundaryError> {
        let mut output = Self::default();
        for (path, source) in &self.0 {
            match source {
                StreamOrigin::Error | StreamOrigin::Opaque | StreamOrigin::Callable(_) | StreamOrigin::Pending(_) | StreamOrigin::OriginalApplication { .. } | StreamOrigin::BinderApplication { .. } | StreamOrigin::DeferredApplication { .. } => { output.0.insert((path.clone(), source.clone())); }
                StreamOrigin::Formal { parameter, projection, .. } | StreamOrigin::OpaqueFormal { parameter, projection, .. } => {
                    let argument = arguments.get(parameter).cloned().unwrap_or_else(Self::opaque);
                    let mut value = argument.project(&projection.0)?;
                    if matches!(source, StreamOrigin::OpaqueFormal { .. }) && !value.0.is_empty() { value = Self::opaque(); }
                    for step in path.0.iter().rev() { value = value.prefix(step.clone())?; }
                    output.join(&value);
                }
            }
        }
        Ok(output)
    }

    pub(super) fn may_fail(&self) -> bool {
        self.0.iter().any(|(path, source)| path.0.is_empty() && matches!(source, StreamOrigin::Error | StreamOrigin::Opaque | StreamOrigin::Formal { .. } | StreamOrigin::OpaqueFormal { .. } | StreamOrigin::OriginalApplication { .. } | StreamOrigin::BinderApplication { .. } | StreamOrigin::DeferredApplication { .. }))
    }

    fn has_pending(&self) -> bool {
        self.0.iter().any(|(_, origin)| matches!(origin, StreamOrigin::Pending(_)))
    }

    fn settle_pending(&mut self, declarations: &BTreeSet<EffectDeclarationId>) {
        self.0.retain(|(_, origin)| !matches!(origin, StreamOrigin::Pending(target) if declarations.contains(target)));
    }

    fn validate_storage(&self, domain: &ProjectionDomain) -> Result<Self, StreamOriginBoundaryError> {
        let mut output = Self::default();
        for (path, origin) in &self.0 {
            match domain.boundary(path)? {
                Some(prefix) => { output.0.insert((prefix, StreamOrigin::Opaque)); }
                None => { output.0.insert((path.clone(), origin.clone())); }
            }
        }
        Ok(output)
    }

    fn validate(&self, owner: EffectDeclarationId, output_domain: &ProjectionDomain, parameters: &[ProjectionDomain]) -> Result<Self, StreamOriginBoundaryError> {
        let mut output = Self::default();
        for (path, source) in &self.0 {
            if let Some(prefix) = output_domain.boundary(path)? {
                output.0.insert((prefix, StreamOrigin::Opaque));
                continue;
            }
            let source = match source {
                StreamOrigin::OriginalApplication { .. } => source.clone(),
                StreamOrigin::DeferredApplication { owner: actual_owner, .. } => {
                    if *actual_owner != owner { return Err(StreamOriginBoundaryError); }
                    source.clone()
                }
                StreamOrigin::BinderApplication { owner: actual_owner, .. } => {
                    if *actual_owner != owner { return Err(StreamOriginBoundaryError); }
                    source.clone()
                }
                StreamOrigin::Formal { owner: source_owner, parameter, projection }
                | StreamOrigin::OpaqueFormal { owner: source_owner, parameter, projection } => {
                    if *source_owner != owner { output.0.insert((path.clone(), StreamOrigin::Opaque)); continue; }
                    let domain = parameters.get(*parameter).ok_or(StreamOriginBoundaryError)?;
                    match domain.boundary(projection)? {
                        Some(prefix) => StreamOrigin::OpaqueFormal { owner, parameter: *parameter, projection: prefix },
                        None => source.clone(),
                    }
                }
                _ => source.clone(),
            };
            output.0.insert((path.clone(), source));
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::{StreamOrigins, StreamProjection};
    use std::collections::BTreeMap;

    #[test]
    fn rejected_declarations_remove_only_their_stream_facts() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let body = crate::source::Span::new(crate::source::SourceId::new(0), 0, 1);
            let owners = ["first", "second"].map(|name| super::EffectDeclarationId { namespace: Some(crate::symbol::Name::intern(name)), body });
            let domain = super::ProjectionDomain::from_type(&super::Type::Stream(Box::new(super::Type::Int))).unwrap();
            let mut summaries = super::StreamOriginSummaries::new();
            for owner in owners {
                let returned = StreamOrigins::formal(owner, 0);
                assert_eq!(returned.validate(owner, &domain, &[domain.clone()]).unwrap(), returned);
                summaries.declarations.insert(owner, super::StreamReturnSummary {
                    returned, defaults: BTreeMap::new(), output: domain.clone(), parameters: vec![domain.clone()],
                });
            }
            for owner in [Some(owners[0]), Some(owners[1]), None] {
                summaries.loops.insert(super::StreamLoopId {
                    owner, namespace: None, block: crate::syntax::arena::BlockId::from_index(0), body,
                }, BTreeMap::from([(super::super::BindingId::args(), StreamOrigins::error())]));
            }
            let original = summaries.clone();
            summaries.reject_declarations(&Default::default());
            assert_eq!(summaries, original);
            summaries.reject_declarations(&[owners[0]].into_iter().collect());
            assert_eq!(summaries.declarations.len(), 1);
            assert_eq!(summaries.declarations[&owners[1]], original.declarations[&owners[1]]);
            assert_eq!(summaries.loops.len(), 2);
            for (identity, header) in &summaries.loops {
                assert_ne!(identity.owner, Some(owners[0]));
                assert_eq!(header, &original.loops[identity]);
            }
            let retained = summaries.clone();
            summaries.reject_declarations(&[owners[0]].into_iter().collect());
            assert_eq!(summaries, retained);
            summaries.reject_declarations(&[owners[1]].into_iter().collect());
            assert!(summaries.declarations.is_empty());
            assert_eq!(summaries.loops.len(), 1);
            let (identity, header) = summaries.loops.first_key_value().unwrap();
            assert_eq!(identity.owner, None);
            assert_eq!(header, &original.loops[identity]);
        });
    }

    #[test]
    fn missing_projected_actual_remains_unknown() {
        let dependency = StreamOrigins::formal(super::EffectDeclarationId { namespace: None, body: crate::source::Span::new(crate::source::SourceId::new(0), 0, 0) }, 0).project(&[StreamProjection::Success]).unwrap();
        assert!(dependency.substitute(&BTreeMap::new()).unwrap().may_fail());
        assert!(StreamOrigins::opaque().project(&[StreamProjection::Success, StreamProjection::Element]).unwrap().may_fail());
    }

    #[test]
    fn callable_requirement_return_origins_distinguish_whole_results_from_data() {
        crate::symbol::SymbolOwner::new().with_current(|| {
        let owner = super::EffectDeclarationId { namespace: None, body: crate::source::Span::new(crate::source::SourceId::new(0), 0, 1) };
        let reference = crate::sema::constraints::CallableApplicationReference {
            scope: crate::sema::constraints::CallableApplicationScope::Declaration(owner), ordinal: 0,
        };
        let application = StreamOrigins::application(reference);
        assert_eq!(application.return_applications(), [Some(reference)].into_iter().collect());
        let field = crate::symbol::Name::intern("item");
        let wrapped = application.prefix(StreamProjection::Field(field)).unwrap();
        assert_eq!(wrapped.return_applications(), [None].into_iter().collect());
        assert_eq!(wrapped.project(&[StreamProjection::Field(field)]).unwrap(), application);
        for projection in [StreamProjection::Success, StreamProjection::Element, StreamProjection::Field(field)] {
            assert_eq!(application.project(&[projection]).unwrap().return_applications(), [None].into_iter().collect());
        }
        let mut mixed = application;
        mixed.join(&StreamOrigins::formal(owner, 0));
        assert_eq!(mixed.return_applications(), [None, Some(reference)].into_iter().collect());
        assert!(mixed.may_fail());
        });
    }

    #[test]
    fn proc_record_completions_keep_their_result_envelope() {
        for completion in [
            "{actual}",
            "return {actual}",
            "if flag { return {actual} }; {actual}",
            "return if flag { {actual} } else { {actual} }",
            "return match flag { true => {actual}; false => {actual} }",
        ] {
            let source = format!(
                "proc side(value: Int) [error] -> Int {{ assert value > 0; value }}\n\
                 proc relay(action, value) {{ action(value) }}\n\
                 proc number(flag: Bool) [error] {{ let actual = relay(side, 7); {completion} }}\n"
            );
            let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), &source);
            assert!(parsed.diagnostics.is_empty(), "{source}\n{:?}", parsed.diagnostics);
            let _symbols = parsed.arena.symbol_owner().enter();
            let checked = super::Checker::check_arena(&parsed.arena, &source);
            assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", checked.diagnostics);
            let declaration = checked.checked_declarations.functions.values()
                .find(|declaration| declaration.name.as_str().as_str() == "number").unwrap();
            let owner = super::EffectDeclarationId { namespace: None, body: declaration.body };
            let result = &checked.function_return_types[&declaration.body];
            let super::Type::Result(ok, _) = result else { panic!("the Proc retains its Result envelope: {result:?}") };
            let super::Type::Record(fields) = ok.as_ref() else { panic!("the success value retains its record: {result:?}") };
            let actual = crate::symbol::Name::intern("actual");
            assert_eq!(fields.get(&actual), Some(&super::Type::Result(Box::new(super::Type::Int), Box::new(super::Type::Error))));
            let summary = &checked.stream_origin_summaries.declarations[&owner];
            assert!(!summary.returned.0.is_empty(), "the returned call retains its source dependency");
            for (path, _) in &summary.returned.0 {
                assert!(path.0.starts_with(&[StreamProjection::Success, StreamProjection::Field(actual)]), "{source}\n{summary:?}");
            }
        }
    }

    #[test]
    fn projection_construction_preserves_the_type_depth_bound() {
        let mut value = StreamOrigins::error();
        for _ in 0..crate::sema::constraints::MAX_TYPE_DEPTH { value = value.prefix(StreamProjection::Success).unwrap(); }
        assert!(value.prefix(StreamProjection::Success).is_err());
    }
}

use super::{Checker, CheckedResultConstructor, Span};
use crate::syntax::arena::{ArenaProgram, ArenaExprKind, ArenaStmtKind, ArenaExprOrRun, ArenaCallArgKind, ExprId, StmtId, BlockId};

impl Checker {
    pub(super) fn resolve_stream_origin_applications(&mut self,
        members: &BTreeMap<EffectDeclarationId, crate::syntax::arena::FunctionDefId>, calls: &BTreeMap<Span, super::PendingCallInstantiation>,
    ) -> Result<(), (EffectDeclarationId, StreamOriginBoundaryError)> {
        let mut resolved = self.stream_origin_outputs.clone();
        for (&owner, summary) in &mut resolved.declarations {
            if !members.contains_key(&owner) { continue; }
            summary.returned = summary.returned.resolve_applications(owner, &self.type_constraints, calls, &self.provisional_function_calls).map_err(|error| (owner, error))?;
            for origin in summary.defaults.values_mut() {
                *origin = origin.resolve_applications(owner, &self.type_constraints, calls, &self.provisional_function_calls).map_err(|error| (owner, error))?;
            }
        }
        for (identity, header) in &mut resolved.loops {
            let Some(owner) = identity.owner.filter(|owner| members.contains_key(owner)) else { continue };
            for origin in header.values_mut() {
                *origin = origin.resolve_applications(owner, &self.type_constraints, calls, &self.provisional_function_calls).map_err(|error| (owner, error))?;
            }
        }
        self.stream_origin_outputs = resolved;
        Ok(())
    }

    pub(super) fn normalize_stream_origin_applications(&mut self) -> Result<(), (EffectDeclarationId, StreamOriginBoundaryError)> {
        let mut normalized = self.stream_origin_outputs.clone();
        for (&owner, summary) in &mut normalized.declarations {
            let pending = self.declaration_inference.pending(super::FunctionDeclarationId::new(owner));
            summary.returned = summary.returned.normalize_applications(owner, &self.type_constraints, pending).map_err(|error| (owner, error))?;
            for origin in summary.defaults.values_mut() {
                *origin = origin.normalize_applications(owner, &self.type_constraints, pending).map_err(|error| (owner, error))?;
            }
        }
        for (identity, header) in &mut normalized.loops {
            let owner = identity.owner.unwrap_or(EffectDeclarationId { namespace: identity.namespace, body: identity.body });
            let pending = identity.owner.and_then(|owner| self.declaration_inference.pending(super::FunctionDeclarationId::new(owner)));
            for origin in header.values_mut() {
                *origin = origin.normalize_applications(owner, &self.type_constraints, pending).map_err(|error| (owner, error))?;
            }
        }
        self.stream_origin_outputs = normalized;
        Ok(())
    }

    pub(super) fn validate_published_stream_origins(&self) -> Result<(), (EffectDeclarationId, StreamOriginBoundaryError)> {
        for (&owner, summary) in &self.stream_origin_outputs.declarations {
            if summary.returned.has_unresolved_applications() || summary.defaults.values().any(StreamOrigins::has_unresolved_applications) {
                return Err((owner, StreamOriginBoundaryError));
            }
        }
        for (identity, header) in &self.stream_origin_outputs.loops {
            if header.values().any(StreamOrigins::has_unresolved_applications) {
                return Err((identity.owner.unwrap_or(EffectDeclarationId { namespace: identity.namespace, body: identity.body }), StreamOriginBoundaryError));
            }
        }
        Ok(())
    }

    pub(super) fn project_stream_origin(&mut self, origin: &StreamOrigins, path: &[StreamProjection], span: Span) -> StreamOrigins {
        match origin.project(path) {
            Ok(origin) => origin,
            Err(_) => { self.report_stream_origin_boundary(span); StreamOrigins::opaque() }
        }
    }

    pub(super) fn stream_success_origin(&mut self, arena: &ArenaProgram, expression: ExprId, ty: &Type) -> StreamOrigins {
        let origin = self.stream_expr_origin(arena, expression);
        if ty.is_result() { self.project_stream_origin(&origin, &[StreamProjection::Success], arena.arena.expr(expression).span) } else { origin }
    }

    pub(super) fn stream_binding_initializer(&mut self, arena: &ArenaProgram, initializer: ArenaExprOrRun, binding_type: &Type) -> StreamOrigins {
        match initializer {
            ArenaExprOrRun::Expr(expression) => {
                let ty = self.expr_types.get(&arena.arena.expr(expression).span).cloned().unwrap_or(Type::Unknown);
                self.stream_success_origin(arena, expression, &ty)
            }
            ArenaExprOrRun::Run(_) => StreamOrigins::unproven(binding_type),
        }
    }

    pub(super) fn stream_iteration_origin(&mut self, arena: &ArenaProgram, expression: ExprId, ty: &Type) -> StreamOrigins {
        let origin = self.stream_success_origin(arena, expression, ty);
        let item = self.project_stream_origin(&origin, &[StreamProjection::Element], arena.arena.expr(expression).span);
        if matches!(ty.result_ok().unwrap_or(ty).unvalidated(), Type::Map(_, _)) {
            match item.prefix(StreamProjection::Field(Name::intern("value"))) {
                Ok(item) => item,
                Err(_) => { self.report_stream_origin_boundary(arena.arena.expr(expression).span); StreamOrigins::opaque() }
            }
        } else { item }
    }

    pub(super) fn report_stream_origin_boundary(&mut self, span: Span) {
        self.error(span, "stream return provenance exceeds its checked projection boundary", crate::diagnostic::DiagnosticCode::CheckLocalInference);
    }

    pub(super) fn stream_expr_origin(&self, arena: &ArenaProgram, expression: ExprId) -> StreamOrigins {
        let span = arena.arena.expr(expression).span;
        self.stream_expr_origins.get(&span).cloned().unwrap_or_else(|| {
            StreamOrigins::unproven(self.expr_types.get(&span).unwrap_or(&Type::Unknown))
        })
    }

    pub(super) fn stream_block_origin(&self, arena: &ArenaProgram, block: BlockId) -> StreamOrigins {
        let Some(statement) = arena.arena.stmt_ids(arena.arena.block(block).statements).last() else { return StreamOrigins::default() };
        let span = arena.arena.stmt(arena.arena.core_stmt_id(statement)).span;
        self.stream_stmt_origins.get(&span).cloned().unwrap_or_else(|| StreamOrigins::unproven(self.expr_types.get(&span).unwrap_or(&Type::Unknown)))
    }

    pub(super) fn record_stream_parameter_default(&mut self, arena: &ArenaProgram, definition: &crate::syntax::arena::ArenaFunctionDef, parameter: usize, expression: ExprId) {
        let owner = self.effect_declaration_id(arena, definition.body);
        let origin = self.stream_expr_origin(arena, expression);
        self.stream_origin_outputs.declarations.entry(owner).or_default().defaults.insert(parameter, origin);
    }

    pub(super) fn record_stream_producer_error(&mut self, owner: EffectDeclarationId) {
        self.stream_origin_outputs.declarations.entry(owner).or_default().returned.join(&StreamOrigins::error());
    }

    pub(super) fn settle_stream_origin_dependencies(&mut self, external: &StreamOriginSummaries) {
        let declarations = external.declarations.keys().chain(self.stream_origin_outputs.declarations.keys()).copied().collect::<BTreeSet<_>>();
        for summary in self.stream_origin_outputs.declarations.values_mut() {
            summary.returned.settle_pending(&declarations);
            for origin in summary.defaults.values_mut() { origin.settle_pending(&declarations); }
        }
        for header in self.stream_origin_outputs.loops.values_mut() {
            for origin in header.values_mut() { origin.settle_pending(&declarations); }
        }
    }

    fn stream_origin_id(&self, arena: &ArenaProgram, target: super::CheckedFunctionTarget) -> EffectDeclarationId {
        let definition = arena.arena.function_def(target.definition);
        EffectDeclarationId { namespace: target.namespace, body: arena.arena.span(arena.arena.block(definition.body).span) }
    }

    fn opaque_stream_value(ty: &Type) -> Result<StreamOrigins, StreamOriginBoundaryError> {
        match ty {
            Type::Result(ok, _) => Self::opaque_stream_value(ok)?.prefix(StreamProjection::Success),
            Type::Optional(value) => Self::opaque_stream_value(value),
            Type::Validated(value) => Self::opaque_stream_value(value.base()),
            value if value.may_hold_resource() => Ok(StreamOrigins::opaque()),
            _ => Ok(StreamOrigins::default()),
        }
    }

    fn stream_call_actuals(&self, arena: &ArenaProgram, args: super::ArenaRange, span: Span, target: EffectDeclarationId, summary: &StreamReturnSummary) -> Result<BTreeMap<usize, StreamOrigins>, StreamOriginBoundaryError> {
        use crate::sema::arguments::{ArgumentValueSource, expand_named_arguments};
        let entries = arena.arena.call_args(args);
        let expanded = expand_named_arguments(arena, entries, |expression| self.expr_types.get(&arena.arena.expr(expression).span).cloned());
        let Ok(expanded) = expanded else { return Ok(BTreeMap::new()) };
        let slots = self.argument_bindings.get(&span);
        let definition = self.stream_target_definitions.get(&target).map(|target| arena.arena.function_def(target.definition));
        let params = definition.map(|definition| arena.arena.params(definition.params));
        let mut output = BTreeMap::<usize, StreamOrigins>::new();
        for (position, argument) in expanded.iter().enumerate() {
            let slot = slots.and_then(|slots| slots.argument_slots.get(position).copied()).unwrap_or_else(|| {
                params.and_then(|params| params.last().filter(|param| param.rest).map(|_| position.min(params.len() - 1))).unwrap_or(position)
            });
            let rest = params.and_then(|params| params.get(slot)).is_some_and(|param| param.rest);
            let origin = match argument.value {
                ArgumentValueSource::Expression(expression) => {
                    let origin = self.stream_expr_origin(arena, expression);
                    if rest { origin.prefix(StreamProjection::Element)? } else { origin }
                }
                ArgumentValueSource::RecordField { record, field } => self.stream_expr_origin(arena, record).project(&[StreamProjection::Field(field)])?,
                ArgumentValueSource::PositionalSplice(expression) => self.stream_expr_origin(arena, expression),
                ArgumentValueSource::CheckedType => StreamOrigins::opaque(),
            };
            output.entry(slot).or_default().join(&origin);
        }
        for (&position, origin) in &summary.defaults { output.entry(position).or_insert_with(|| origin.clone()); }
        Ok(output)
    }

    pub(super) fn record_checked_stream_origin(&mut self, arena: &ArenaProgram, expression: ExprId, ty: &Type) {
        let span = arena.arena.expr(expression).span;
        let result = self.derive_checked_stream_origin(arena, expression, ty);
        let origin = match result {
            Ok(origin) => origin,
            Err(_) => { self.report_stream_origin_boundary(span); StreamOrigins::opaque() }
        };
        if !self.collecting_effects && self.authoritative_effect_graph && origin.has_pending() && ty.may_hold_resource() {
            self.error(span, "selected declaration has no settled Stream return provenance", crate::diagnostic::DiagnosticCode::CheckLocalInference);
        }
        self.stream_expr_origins.insert(span, origin);
    }

    fn derive_checked_stream_origin(&mut self, arena: &ArenaProgram, expression: ExprId, ty: &Type) -> Result<StreamOrigins, StreamOriginBoundaryError> {
        let span = arena.arena.expr(expression).span;
        if !ty.may_hold_resource() || self.builtin_range_calls.contains(&span) { return Ok(StreamOrigins::default()); }
        let application_scope = self.stream_origin_owner.map_or(crate::sema::constraints::CallableApplicationScope::Invocation, crate::sema::constraints::CallableApplicationScope::Declaration);
        if let Some(reference) = self.type_constraints.source_callable_application_reference(application_scope, span).map_err(|_| StreamOriginBoundaryError)? {
            return Ok(StreamOrigins::application(reference));
        }
        if self.selected_function_value_references.contains(&span)
            && let Some(target) = self.selected_function_calls.get(&span).copied()
        {
            let id = self.stream_origin_id(arena, target);
            self.stream_target_definitions.insert(id, target);
            return Ok(StreamOrigins::callable(id));
        }
        let origin = match arena.arena.expr(expression).kind {
            ArenaExprKind::Ident(_) | ArenaExprKind::Item => self.lookup_value_binding_for_expr(arena, expression).binding().map(|binding| binding.stream_origins.clone()),
            ArenaExprKind::List(elements) => {
                let mut origin = StreamOrigins::default();
                for element in arena.arena.list_elements(elements) {
                    let value = self.stream_expr_origin(arena, element.value);
                    origin.join(&if element.splice_span.is_some() { value } else { value.prefix(StreamProjection::Element)? });
                }
                Some(origin)
            }
            ArenaExprKind::Record(fields) => {
                let mut origin = StreamOrigins::default();
                for field in arena.arena.record_fields(fields) {
                    use crate::syntax::arena::ArenaRecordFieldKind as Field;
                    let value = match &field.kind {
                        Field::Spread { expr, .. } => self.stream_expr_origin(arena, *expr),
                        Field::Named { name, value, .. } => {
                            let step = if matches!(ty.unvalidated(), Type::Map(_, _)) { StreamProjection::Element } else { StreamProjection::Field(*name) };
                            self.stream_expr_origin(arena, *value).prefix(step)?
                        }
                        Field::Shorthand { name, span } => {
                            let value = self.lookup_value_binding_at(*name, arena.arena.span(*span)).binding().map(|binding| binding.stream_origins.clone()).unwrap_or_else(StreamOrigins::opaque);
                            let step = if matches!(ty.unvalidated(), Type::Map(_, _)) { StreamProjection::Element } else { StreamProjection::Field(*name) };
                            value.prefix(step)?
                        }
                        Field::Computed { value, .. } => self.stream_expr_origin(arena, *value).prefix(StreamProjection::Element)?,
                        Field::Path { path, value, .. } => {
                            let mut value = self.stream_expr_origin(arena, *value);
                            for name in arena.arena.names(*path).collect::<Vec<_>>().into_iter().rev() { value = value.prefix(StreamProjection::Field(name))?; }
                            value
                        }
                    };
                    origin.join(&value);
                }
                Some(origin)
            }
            ArenaExprKind::Try(value) => Some(self.stream_expr_origin(arena, value).project(&[StreamProjection::Success])?),
            ArenaExprKind::Require { value, .. } | ArenaExprKind::Convert { value, .. } => Some(self.stream_expr_origin(arena, value)),
            ArenaExprKind::Field { base, name } | ArenaExprKind::NullSafeField { base, name } => Some(self.stream_expr_origin(arena, base).project(&[StreamProjection::Field(name)])?),
            ArenaExprKind::Index { base, .. } => Some(self.stream_expr_origin(arena, base).project(&[StreamProjection::Element])?),
            ArenaExprKind::ValueBlock(block) | ArenaExprKind::ErrorContext { block, .. } => Some(self.stream_block_origin(arena, block)),
            ArenaExprKind::Capture(block) => Some(self.stream_block_origin(arena, block).prefix(StreamProjection::Success)?),
            ArenaExprKind::If { branches, else_value } => {
                let mut origin = self.stream_expr_origin(arena, else_value);
                for branch in arena.arena.if_expr_branches(branches) { origin.join(&self.stream_expr_origin(arena, branch.value)); }
                Some(origin)
            }
            ArenaExprKind::Match { arms, .. } => {
                let mut origin = StreamOrigins::default();
                for arm in arena.arena.match_expr_arms(arms) { origin.join(&self.stream_expr_origin(arena, arm.value)); }
                Some(origin)
            }
            ArenaExprKind::Call { callee, args } => {
                if self.result_constructors.get(&span) == Some(&CheckedResultConstructor::Ok) {
                    let argument = arena.arena.call_args(args).first().and_then(|argument| match argument.kind {
                        ArenaCallArgKind::Positional(value) | ArenaCallArgKind::Named { value, .. } => Some(value),
                        _ => None,
                    });
                    Some(argument.map(|value| self.stream_expr_origin(arena, value)).unwrap_or_default().prefix(StreamProjection::Success)?)
                } else if self.result_constructors.get(&span) == Some(&CheckedResultConstructor::Err) {
                    Some(StreamOrigins::default())
                } else {
                    let callee_origin = self.stream_expr_origin(arena, callee);
                    let mut targets = callee_origin.targets().collect::<BTreeSet<_>>();
                    if let Some(target) = self.selected_function_calls.get(&span).copied() {
                        let id = self.stream_origin_id(arena, target);
                        self.stream_target_definitions.insert(id, target);
                        targets.clear();
                        targets.insert(id);
                    }
                    if targets.is_empty() { None } else {
                        let mut origin = if !self.selected_function_calls.contains_key(&span) && callee_origin.has_unknown_callable() { Self::opaque_stream_value(ty)? } else { StreamOrigins::default() };
                        for target in targets {
                            if target.namespace == self.current_namespace { self.provisional_stream_origins_read.set(true); }
                            if let Some(summary) = self.stream_origin_inputs.declarations.get(&target) {
                                let arguments = self.stream_call_actuals(arena, args, span, target, summary)?;
                                let instances = self.pending_call_instantiations.get(&span).map(|(_, instance)| instance.applications.as_slice());
                                let pending = self.declaration_inference.pending(super::FunctionDeclarationId::new(target));
                                let deferred_call = self.provisional_function_calls.get(&span).filter(|(owner, callee)| Some(*owner) == self.stream_origin_owner && callee.effect_declaration() == target).map(|(owner, _)| (*owner, span));
                                let returned = summary.returned.specialize_applications(target, instances, &self.type_constraints, pending, deferred_call)?;
                                origin.join(&returned.substitute(&arguments)?);
                            } else { origin.join(&StreamOrigins::pending(target)); }
                        }
                        Some(origin)
                    }
                }
            }
            _ => None,
        };
        let mut origin = origin.unwrap_or(Self::opaque_stream_value(ty)?);
        if matches!(ty.unvalidated(), Type::Stream(_)) && self.latent_stream_error_spans.contains(&span) { origin.join(&StreamOrigins::error()); }
        Ok(origin)
    }

    pub(super) fn record_checked_stream_tail(&mut self, arena: &ArenaProgram, statement: StmtId, ty: &Type) {
        let statement = arena.arena.core_stmt_id(statement);
        let node = arena.arena.stmt(statement);
        let mut origin = StreamOrigins::default();
        let mut result = ty.is_result();
        match node.kind {
            ArenaStmtKind::Expr(expression) => { origin = self.stream_expr_origin(arena, expression); }
            ArenaStmtKind::TailBareIdent(name) => {
                if let Some(binding) = self.lookup_value_binding_at(name, node.span).binding() {
                    origin = binding.stream_origins.clone();
                    result = binding.ty.is_result();
                } else { origin = StreamOrigins::opaque(); }
            }
            ArenaStmtKind::If { branches, else_block } => {
                for branch in arena.arena.if_branches(branches) { origin.join(&self.stream_block_origin(arena, branch.block)); }
                if let Some(block) = else_block { origin.join(&self.stream_block_origin(arena, block)); }
            }
            ArenaStmtKind::Match { arms, .. } => {
                for arm in arena.arena.match_arms(arms) { origin.join(&self.stream_block_origin(arena, arm.block)); }
            }
            _ => {}
        }
        if self.selected_function_value_references.contains(&node.span)
            && let Some(target) = self.selected_function_calls.get(&node.span).copied()
        {
            let target_id = self.stream_origin_id(arena, target);
            self.stream_target_definitions.insert(target_id, target);
            origin = StreamOrigins::callable(target_id);
        }
        self.stream_stmt_origins.insert(node.span, origin);
        if result { self.stream_stmt_results.insert(node.span); }
        else { self.stream_stmt_results.remove(&node.span); }
    }

    pub(super) fn stream_mutable_state(&self) -> BTreeMap<super::BindingId, StreamOrigins> {
        let mut state = BTreeMap::new();
        for scope in self.scopes.iter().rev() {
            for binding in scope.values().filter(|binding| binding.mutable && binding.ty.may_hold_resource()) {
                if let Some(identity) = binding.id { state.entry(identity).or_insert_with(|| binding.stream_origins.clone()); }
            }
        }
        state
    }

    pub(super) fn restore_stream_mutable_state(&mut self, state: &BTreeMap<super::BindingId, StreamOrigins>) {
        for scope in &mut self.scopes {
            for binding in scope.values_mut() {
                if let Some(origin) = binding.id.and_then(|identity| state.get(&identity)) { binding.stream_origins = origin.clone(); }
            }
        }
    }

    pub(super) fn join_stream_mutable_states(&mut self, states: &[BTreeMap<super::BindingId, StreamOrigins>]) {
        let mut joined = BTreeMap::<_, StreamOrigins>::new();
        for state in states { for (&identity, origin) in state { joined.entry(identity).or_default().join(origin); } }
        self.restore_stream_mutable_state(&joined);
    }

    pub(super) fn enter_stream_loop(&mut self, arena: &ArenaProgram, block: BlockId, kind: StreamLoopKind) {
        let identity = StreamLoopId { owner: self.stream_origin_owner, namespace: self.current_namespace, block, body: arena.arena.span(arena.arena.block(block).span) };
        let mut header = self.stream_mutable_state();
        let enclosing_flow = self.boundary.stream_flow;
        let body_flow = if enclosing_flow == StreamFlow::Live && self.stream_loop_body_may_enter(arena, kind) { StreamFlow::Live } else { StreamFlow::Dead };
        let mut types = BTreeMap::new();
        for scope in self.scopes.iter().rev() {
            for binding in scope.values() {
                if let Some(identity) = binding.id.filter(|identity| header.contains_key(identity)) {
                    types.entry(identity).or_insert_with(|| binding.unrefined_ty.as_ref().unwrap_or(&binding.ty).clone());
                }
            }
        }
        if body_flow == StreamFlow::Live && let Some(previous) = self.stream_origin_inputs.loops.get(&identity) {
            for (&binding, origin) in previous {
                if let Some(current) = header.get_mut(&binding) && current.join(origin) { self.provisional_stream_origins_read.set(true); }
            }
        }
        for (&binding, origin) in &mut header {
            match ProjectionDomain::from_type(&types[&binding]).and_then(|domain| origin.validate_storage(&domain)) {
                Ok(checked) => *origin = checked,
                Err(_) => { self.report_stream_origin_boundary(identity.body); *origin = StreamOrigins::opaque(); }
            }
        }
        self.restore_stream_mutable_state(&header);
        self.boundary.stream_loops.push(StreamLoopFrame { identity, kind, enclosing_flow, body_flow, types, header, breaks: BTreeMap::new() });
        self.boundary.stream_flow = body_flow;
    }

    pub(super) fn record_stream_loop_control(&mut self, transfer: StreamLoopTransfer) {
        if self.boundary.stream_flow == StreamFlow::Dead { return; }
        let state = self.stream_mutable_state();
        let Some(frame) = self.boundary.stream_loops.last_mut() else { return };
        let destination = if matches!(transfer, StreamLoopTransfer::Break) { &mut frame.breaks } else { &mut frame.header };
        for (identity, origin) in state {
            if frame.types.contains_key(&identity) {
                let changed = destination.entry(identity).or_default().join(&origin);
                if changed && matches!(transfer, StreamLoopTransfer::Backedge) { self.provisional_stream_origins_read.set(true); }
            }
        }
    }

    pub(super) fn record_stream_while_condition(&mut self, arena: &ArenaProgram, condition: ExprId) {
        if !self.stream_expr_may_complete(arena, condition) { self.boundary.stream_flow = StreamFlow::Dead; return; }
        let state = self.stream_mutable_state();
        let false_edge = self.stream_condition_may_be_false(arena, condition);
        let Some(frame) = self.boundary.stream_loops.last_mut() else { return };
        if frame.enclosing_flow == StreamFlow::Live && false_edge {
            for (identity, origin) in state {
                if frame.types.contains_key(&identity) { frame.breaks.entry(identity).or_default().join(&origin); }
            }
        }
    }

    pub(super) fn leave_stream_loop(&mut self, arena: &ArenaProgram, block: BlockId) {
        if self.boundary.stream_loops.last().is_some_and(|frame| frame.body_flow == StreamFlow::Live) && self.stream_block_may_complete(arena, block) {
            self.record_stream_loop_control(StreamLoopTransfer::Backedge);
        }
        let Some(mut frame) = self.boundary.stream_loops.pop() else { return };
        for (&binding, origin) in &mut frame.header {
            match ProjectionDomain::from_type(&frame.types[&binding]).and_then(|domain| origin.validate_storage(&domain)) {
                Ok(checked) => *origin = checked,
                Err(_) => { self.report_stream_origin_boundary(frame.identity.body); *origin = StreamOrigins::opaque(); }
            }
        }
        self.stream_origin_outputs.loops.insert(frame.identity, frame.header.clone());
        let mut exits = vec![frame.breaks];
        if !matches!(frame.kind, StreamLoopKind::Loop) { exits.push(frame.header); }
        self.join_stream_mutable_states(&exits);
        self.boundary.stream_flow = frame.enclosing_flow;
    }

    pub(super) fn assign_stream_origin(&mut self, arena: &ArenaProgram, target: crate::syntax::arena::AssignTargetId, selected: &super::Binding, value: ArenaExprOrRun, span: Span) {
        use crate::syntax::arena::ArenaAssignTargetKind as Target;
        let mut current = target;
        let mut path = Vec::new();
        let name = loop {
            if path.len() > crate::sema::constraints::MAX_TYPE_DEPTH { self.report_stream_origin_boundary(span); return; }
            match arena.arena.assign_target(current).kind {
                Target::Name(name) => break name,
                Target::Field { base, name } => { path.push(StreamProjection::Field(name)); current = base; }
                Target::Index { base, .. } => { path.push(StreamProjection::Element); current = base; }
                Target::Env(_) => return,
            }
        };
        path.reverse();
        let value = match value { ArenaExprOrRun::Expr(value) => self.stream_expr_origin(arena, value), ArenaExprOrRun::Run(_) => StreamOrigins::default() };
        let root = selected.unrefined_ty.as_ref().unwrap_or(&selected.ty);
        let origin = match selected.stream_origins.replace_projection(root, &path, &value) {
            Ok(origin) => origin,
            Err(_) => { self.report_stream_origin_boundary(span); return; }
        };
        if let Some(scope) = self.binding_scope(name, selected)
            && let Some(binding) = self.scopes[scope].get_mut(&name)
        { binding.stream_origins = origin; }
    }

    pub(super) fn record_stream_yield_origin(&mut self, origin: StreamOrigins, span: Span) {
        let Some(owner) = self.stream_origin_owner else { return };
        match origin.prefix(StreamProjection::Element) {
            Ok(origin) => { self.stream_origin_outputs.declarations.entry(owner).or_default().returned.join(&origin); }
            Err(_) => self.report_stream_origin_boundary(span),
        }
    }

    pub(super) fn record_stream_delegated_origin(&mut self, arena: &ArenaProgram, expression: ExprId) {
        let Some(owner) = self.stream_origin_owner else { return };
        let origin = self.stream_expr_origin(arena, expression);
        self.stream_origin_outputs.declarations.entry(owner).or_default().returned.join(&origin);
    }

    pub(super) fn record_stream_return(&mut self, arena: &ArenaProgram, value: Option<ArenaExprOrRun>, actual: &Type, expected: &Type, span: Span) {
        let origin = match value { Some(ArenaExprOrRun::Expr(value)) => self.stream_expr_origin(arena, value), _ => StreamOrigins::default() };
        self.record_stream_return_origin(origin, actual, expected, span);
    }

    pub(super) fn record_stream_return_origin(&mut self, mut origin: StreamOrigins, actual: &Type, expected: &Type, span: Span) {
        let Some(owner) = self.stream_origin_owner else { return };
        let expected = self.boundary.proc_completion.as_ref()
            .filter(|completion| completion.owner == owner)
            .map_or(expected, |completion| &completion.return_ty);
        if expected.is_result() && !actual.is_result() {
            match origin.prefix(StreamProjection::Success) { Ok(value) => origin = value, Err(_) => { self.report_stream_origin_boundary(span); return; } }
        }
        self.stream_origin_outputs.declarations.entry(owner).or_default().returned.join(&origin);
    }

    pub(super) fn finish_stream_return_summary(&mut self, arena: &ArenaProgram, definition: &crate::syntax::arena::ArenaFunctionDef, expected: &Type) {
        let owner = self.effect_declaration_id(arena, definition.body);
        let mut origin = self.stream_block_origin(arena, definition.body);
        let tail = arena.arena.stmt_ids(arena.arena.block(definition.body).statements).last().map(|statement| arena.arena.stmt(arena.arena.core_stmt_id(statement)).span);
        if expected.is_result() && tail.is_some_and(|span| !self.stream_stmt_results.contains(&span)) {
            match origin.prefix(StreamProjection::Success) { Ok(value) => origin = value, Err(_) => { self.report_stream_origin_boundary(owner.body); return; } }
        }
        self.finish_stream_summary(arena, definition, expected, origin);
    }

    pub(super) fn finish_stream_producer_summary(&mut self, arena: &ArenaProgram, definition: &crate::syntax::arena::ArenaFunctionDef, expected: &Type) {
        self.finish_stream_summary(arena, definition, expected, StreamOrigins::default());
    }

    fn finish_stream_summary(&mut self, arena: &ArenaProgram, definition: &crate::syntax::arena::ArenaFunctionDef, expected: &Type, origin: StreamOrigins) {
        let owner = self.effect_declaration_id(arena, definition.body);
        let shape = ProjectionDomain::from_type(expected);
        let parameters = arena.arena.params(definition.params).iter().map(|param| {
            let span = arena.arena.span(param.span);
            let ty = self.parameter_types.get(&span).cloned().unwrap_or(Type::Unknown);
            let ty = self.type_constraints.resolve(&ty).unwrap_or(Type::Unknown);
            ProjectionDomain::from_type(&ty)
        }).collect::<Result<Vec<_>, _>>();
        let (Ok(output), Ok(parameters)) = (shape, parameters) else { self.report_stream_origin_boundary(owner.body); return };
        let summary = self.stream_origin_outputs.declarations.entry(owner).or_default();
        summary.returned.join(&origin);
        summary.output = output;
        summary.parameters = parameters;
        match summary.returned.validate(owner, &summary.output, &summary.parameters) {
            Ok(origins) => summary.returned = origins,
            Err(_) => self.report_stream_origin_boundary(owner.body),
        }
    }
}
