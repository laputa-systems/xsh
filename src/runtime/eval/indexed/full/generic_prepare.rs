use super::*;
use super::super::generic::{
    graph_ground_type, CallableKind, ConcreteOperationId, ForwardedRequirement,
    ForwardingPlan, Instantiation, PhysicalLayout, PhysicalLayoutId, QuantifierKind,
    SchemeScope, SolvedArgument, SolvedCall, SolvedRecordLayout, SolvedRequirementUse, TypeTemplate,
};
use crate::sema::check::{ReturnElaboration, SolvedTypes};
use crate::sema::inference::{InferenceContext, RequirementTemplate, SchemeId, TypeId as GraphTypeId, TypeNode, VariableKind};

#[derive(Clone)]
struct PreparedCall {
    instruction: u32,
    owner: InstructionOwner,
    target: SchemeScopeId,
    caller: Option<SchemeScopeId>,
    substitutions: Vec<TypeRef>,
    parameters: Vec<TypeRef>,
    result: TypeRef,
}

struct PreparedForwarding {
    call: PreparedCall,
    requirements: Vec<ForwardedRequirement>,
    edges: Vec<(InstantiationId, InstantiationId)>,
}

fn problem(construct: &'static str) -> IrBuildError {
    IrBuildError::format(construct, None, 0, 0)
}

impl FullBuilder {
    fn reference(&mut self, graph: &InferenceContext, scheme: SchemeId, ty: GraphTypeId) -> Result<TypeRef, IrBuildError> {
        self.generic.get_or_insert_with(GenericEvidenceBuilder::default)
            .prepare_reference(graph, scheme, ty, &mut self.store.semantic, &mut self.semantic)
            .map_err(|_| problem("generic_scoped_type"))
    }

    fn ground(&mut self, graph: &InferenceContext, ty: GraphTypeId) -> Result<TypeId, IrBuildError> {
        let ty = graph_ground_type(graph, ty).map_err(|_| problem("generic_ground_type"))?;
        self.semantic.intern_type(&mut self.store.semantic, &ty)
    }

    pub(super) fn prepare_generic_scope(&mut self, function: IrFunctionId, body: &FunctionBuild) -> Result<Option<SchemeScopeId>, IrBuildError> {
        let Some(identity) = body.solved_declaration else { return Ok(None); };
        let solved = Arc::clone(self.solved.as_ref().ok_or_else(|| problem("generic_missing_solved_owner"))?);
        let declaration = solved.declarations.get(&identity).ok_or_else(|| problem("generic_missing_declaration"))?;
        let graph = &solved.graph;
        let scheme = graph.scheme(declaration.scheme).map_err(|_| problem("generic_foreign_scheme"))?;
        if !scheme.effect_quantifiers.is_empty() { return Err(problem("generic_effect_evidence_not_prepared")); }
        if scheme.quantifiers.is_empty() { return Ok(None); }
        let signature = graph.resolved(declaration.signature).map_err(|_| problem("generic_signature"))?;
        let TypeNode::Arrow(arrow) = graph.node(signature).map_err(|_| problem("generic_signature"))? else { return Err(problem("generic_signature_not_callable")); };
        if arrow.params.len() != body.params.len() { return Err(problem("generic_parameter_arity")); }
        if solved.owner != graph.owner() { return Err(problem("generic_foreign_solved_owner")); }
        for (index, parameter) in arrow.params.iter().enumerate() {
            if parameter.label != body.params[index] || parameter.rest != body.param_rest[index]
                || parameter.defaulted != body.param_defaults[index].is_some()
            { return Err(problem("generic_parameter_contract")); }
        }
        let parameters = arrow.params.iter().map(|parameter| self.reference(graph, declaration.scheme, parameter.ty)).collect::<Result<Vec<_>, _>>()?;
        let result = self.reference(graph, declaration.scheme, arrow.result)?;
        let mut requirements = Vec::new();
        for template in &scheme.requirements {
            let (left, right, result) = match *template {
                RequirementTemplate::Add { left, right, result } => (left, right, result),
                RequirementTemplate::Eligibility { .. } | RequirementTemplate::Operation { .. } | RequirementTemplate::EqualityCompatible { .. } | RequirementTemplate::CallableInvocation { .. } | RequirementTemplate::ErrorJoin { .. } | RequirementTemplate::EffectInclusion { .. } => return Err(problem("generic_runtime_requirement_not_prepared")),
            };
            requirements.push(Requirement::Add {
                left: self.reference(graph, declaration.scheme, left)?,
                right: self.reference(graph, declaration.scheme, right)?,
                result: self.reference(graph, declaration.scheme, result)?,
            });
        }
        // Row obligations travel with parameters even when this body forwards
        // the projection to another declaration rather than reading the field.
        for (parameter, formal) in arrow.params.iter().enumerate() {
            let ty = graph.resolved(formal.ty).map_err(|_| problem("generic_record_parameter"))?;
            if !matches!(graph.node(ty).map_err(|_| problem("generic_record_parameter"))?, TypeNode::Record(_)) { continue; }
            let fields = match parameters[parameter] {
                TypeRef::Ground(ty) => {
                    let (names, types) = self.store.semantic.record_fields(ty).map_err(|_| problem("generic_record_parameter"))?;
                    names.iter().copied().zip(types.iter().copied()).map(|(name, ty)| {
                        TypeId::from_raw(ty).map(|ty| (name, TypeRef::Ground(ty))).ok_or_else(|| problem("generic_record_field_type"))
                    }).collect::<Result<Vec<_>, _>>()?
                }
                TypeRef::Template(id) => {
                    let TypeTemplate::Record { fields, .. } = self.generic.as_ref().unwrap().template(id).map_err(|_| problem("generic_record_template"))? else { return Err(problem("generic_record_template")); };
                    fields.to_vec()
                }
                TypeRef::Rigid(_) => return Err(problem("generic_record_parameter")),
            };
            for (field, result) in fields {
                requirements.push(Requirement::Projection { receiver: parameters[parameter], receiver_parameter: parameter as u32, field, result });
            }
        }
        let mut row_prefixes = BTreeMap::new();
        let mut seen = rustc_hash::FxHashSet::default();
        for &reference in parameters.iter().chain(std::iter::once(&result)) {
            self.collect_row_prefixes(reference, &mut row_prefixes, &mut seen, 0)?;
        }
        for requirement in &requirements {
            match *requirement {
                Requirement::Add { left, right, result } => for reference in [left, right, result] { self.collect_row_prefixes(reference, &mut row_prefixes, &mut seen, 0)?; },
                Requirement::Projection { receiver, result, .. } => for reference in [receiver, result] { self.collect_row_prefixes(reference, &mut row_prefixes, &mut seen, 0)?; },
            }
        }
        for (index, quantifier) in scheme.quantifiers.iter().enumerate() {
            if quantifier.lacks.iter().any(|label| !row_prefixes.get(&(index as u32)).is_some_and(|labels| labels.contains(label))) {
                return Err(problem("generic_row_lacks_evidence_not_prepared"));
            }
        }
        let kind = match declaration.kind {
            crate::sema::inference::CallableKind::Pure => CallableKind::Pure,
            crate::sema::inference::CallableKind::Proc => CallableKind::Proc,
            crate::sema::inference::CallableKind::Stream => return Err(problem("generic_stream_scope")),
        };
        let return_plan = match declaration.return_elaboration {
            ReturnElaboration::Value => GenericReturnPlan::Value,
            ReturnElaboration::ImplicitResult => GenericReturnPlan::Result,
            ReturnElaboration::UnitConsuming => {
                let result = graph.resolved(arrow.result).map_err(|_| problem("generic_return_plan"))?;
                if matches!(graph.node(result).map_err(|_| problem("generic_return_plan"))?, TypeNode::Result(_, _)) { GenericReturnPlan::ResultUnit }
                else { GenericReturnPlan::Unit }
            }
        };
        let flags = (0..body.params.len()).map(|index| {
            u8::from(body.param_rest[index]) | u8::from(body.param_defaults[index].is_some()) << 1
                | u8::from(matches!(body.param_defaults[index], Some(LoweredValue::OmittedArgument))) << 2
        }).collect::<Vec<_>>();
        let scope = self.generic_evidence_mut().add_scope(SchemeScope {
            owner: function, quantifiers: scheme.quantifiers.iter().map(|quantifier| match quantifier.kind {
                VariableKind::Type => QuantifierKind::Type, VariableKind::Row => QuantifierKind::Row,
            }).collect::<Vec<_>>().into_boxed_slice(),
            parameters: parameters.into_boxed_slice(), parameter_names: body.params.clone().into_vec().into_boxed_slice(),
            parameter_flags: flags.into_boxed_slice(), kind, result,
            requirements: requirements.into_boxed_slice(), return_plan,
        }).map_err(|_| problem("generic_scope_allocation"))?;
        self.generic_declarations.insert(identity, scope);
        self.generic_schemes.insert(scope, declaration.scheme);
        let original = self.generic.as_ref().unwrap().scope(scope).map_err(|_| problem("generic_scope_allocation"))?.clone();
        for (expression, projection) in &solved.projections {
            if solved.expression_owners.get(expression) != Some(&identity) { continue; }
            let scratch = body.scratch.borrow();
            let parameter = body.expression_origins.iter().find_map(|(id, origin)| {
                if origin != expression { return None; }
                let BuildExprRow::Field { base, .. } = scratch.expressions.get(id.index())? else { return None; };
                let BuildExprRow::Param(slot) = scratch.expressions.get(base.index())? else { return None; };
                Some(*slot)
            }).ok_or_else(|| problem("generic_projection_requires_physical_source"))?;
            let requirement = original.requirements.iter().position(|requirement| matches!(requirement,
                Requirement::Projection { receiver_parameter, field, .. } if *receiver_parameter as usize == parameter && *field == projection.field))
                .ok_or_else(|| problem("generic_projection_requirement"))?;
            self.generic_projection_uses.insert(*expression, (scope, requirement as u32));
        }
        Ok(Some(scope))
    }

    fn collect_row_prefixes(&self, reference: TypeRef, prefixes: &mut BTreeMap<u32, std::collections::BTreeSet<Name>>, seen: &mut rustc_hash::FxHashSet<TypeRef>, depth: usize) -> Result<(), IrBuildError> {
        if depth >= 512 { return Err(problem("generic_row_lacks_type_depth")); }
        if !seen.insert(reference) { return Ok(()); }
        let TypeRef::Template(id) = reference else { return Ok(()); };
        let template = self.generic.as_ref().unwrap().template(id).map_err(|_| problem("generic_row_lacks_template"))?;
        match template {
            TypeTemplate::Optional(inner) | TypeTemplate::List(inner) | TypeTemplate::Stream(inner) => self.collect_row_prefixes(*inner, prefixes, seen, depth + 1)?,
            TypeTemplate::Map { key, value } => for reference in [*key, *value] { self.collect_row_prefixes(reference, prefixes, seen, depth + 1)?; },
            TypeTemplate::Result { ok, error } => for reference in [*ok, *error] { self.collect_row_prefixes(reference, prefixes, seen, depth + 1)?; },
            TypeTemplate::Record { fields, row_tail } => {
                if let Some(index) = row_tail { prefixes.entry(*index).or_default().extend(fields.iter().map(|field| field.0)); }
                for &(_, reference) in fields { self.collect_row_prefixes(reference, prefixes, seen, depth + 1)?; }
            }
            TypeTemplate::Arrow { parameters, result, .. } => {
                for parameter in parameters { self.collect_row_prefixes(parameter.ty, prefixes, seen, depth + 1)?; }
                self.collect_row_prefixes(*result, prefixes, seen, depth + 1)?;
            }
        }
        Ok(())
    }

    fn layout(&mut self, ty: TypeId, layouts: &mut BTreeMap<TypeId, PhysicalLayoutId>) -> Result<PhysicalLayoutId, IrBuildError> {
        if let Some(&layout) = layouts.get(&ty) { return Ok(layout); }
        let (names, types) = self.store.semantic.record_fields(ty).map_err(|_| problem("generic_projection_not_record"))?;
        let fields = names.iter().copied().zip(types.iter().copied()).map(|(name, ty)| {
            TypeId::from_raw(ty).map(|ty| (name, ty)).ok_or_else(|| problem("generic_record_field_type"))
        }).collect::<Result<Vec<_>, _>>()?;
        let id = self.generic_evidence_mut().add_layout(PhysicalLayout { record_type: ty, fields: fields.into_boxed_slice() })
            .map_err(|_| problem("generic_layout_allocation"))?;
        layouts.insert(ty, id);
        Ok(id)
    }

    pub(super) fn prepare_generic_expressions(&mut self) -> Result<(), IrBuildError> {
        if self.generic_declarations.is_empty() { return Ok(()); }
        let solved = Arc::clone(self.solved.as_ref().ok_or_else(|| problem("generic_missing_solved_owner"))?);
        let mut layouts = BTreeMap::new();
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            let tag = *self.store.tags.get(instruction as usize).ok_or_else(|| problem("generic_expression_instruction"))?;
            if tag == FullTag::ExprRecord {
                if let Some(&ty) = solved.expressions.get(&expression) {
                    if let Ok(ty) = self.ground(&solved.graph, ty) {
                        let layout = self.layout(ty, &mut layouts)?;
                        self.generic_evidence_mut().add_constructor(SolvedRecordLayout { instruction, owner, layout });
                    }
                }
            }
            if let Some(&(scope, requirement)) = self.generic_projection_uses.get(&expression) {
                self.generic_evidence_mut().add_requirement_use(SolvedRequirementUse { instruction, scope, requirement });
            }
            if let Some(&requirement) = solved.additions.get(&expression) {
                if let Some(declaration) = solved.expression_owners.get(&expression) {
                    if let Some(&scope) = self.generic_declarations.get(declaration) {
                        let scheme = self.generic_schemes[&scope];
                        let (left, right, result) = match solved.graph.requirement_template(requirement).map_err(|_| problem("generic_add_requirement"))? {
                            RequirementTemplate::Add { left, right, result } => (left, right, result),
                            RequirementTemplate::Eligibility { .. } | RequirementTemplate::Operation { .. } | RequirementTemplate::EqualityCompatible { .. } | RequirementTemplate::CallableInvocation { .. } | RequirementTemplate::ErrorJoin { .. } | RequirementTemplate::EffectInclusion { .. } => return Err(problem("generic_runtime_requirement_not_prepared")),
                        };
                        let pending = solved.graph.scheme(scheme).map_err(|_| problem("generic_add_scope"))?.requirement_origins.iter().position(|origin| *origin == requirement);
                        if let Some(index) = pending {
                            let expected = Requirement::Add { left: self.reference(&solved.graph, scheme, left)?, right: self.reference(&solved.graph, scheme, right)?, result: self.reference(&solved.graph, scheme, result)? };
                            if self.generic.as_ref().unwrap().scope(scope).map_err(|_| problem("generic_add_scope"))?.requirements.get(index) != Some(&expected) {
                                return Err(problem("generic_add_body_requirement"));
                            }
                            self.generic_evidence_mut().add_requirement_use(SolvedRequirementUse { instruction, scope, requirement: index as u32 });
                        } else {
                            // A definition can fix this operation while its other
                            // obligations remain generic. Its own checked discharge
                            // proves the fixed instruction, without a frame guard.
                            let evidence = solved.graph.discharge(requirement).map_err(|_| problem("generic_add_requirement"))?
                                .ok_or_else(|| problem("generic_add_body_requirement"))?;
                            if solved.owner != solved.graph.owner() || evidence.requirement != requirement
                                || !matches!(evidence.operation, crate::sema::inference::SealedOperation::AddInt | crate::sema::inference::SealedOperation::AddFloat | crate::sema::inference::SealedOperation::AddStr) {
                                return Err(problem("generic_add_body_requirement"));
                            }
                            for (source, checked) in [(left, evidence.left), (right, evidence.right), (result, evidence.result)] {
                                if solved.graph.resolved(source).map_err(|_| problem("generic_add_requirement"))? != solved.graph.resolved(checked).map_err(|_| problem("generic_add_requirement"))? {
                                    return Err(problem("generic_add_body_requirement"));
                                }
                                self.ground(&solved.graph, checked)?;
                            }
                        }
                    }
                }
            }
        }
        self.prepare_generic_calls(&solved, &mut layouts)
    }

    fn encoded_call_sources(&self, instruction: u32, count: usize, target: SchemeScopeId) -> Result<Vec<Option<u32>>, IrBuildError> {
        let words = self.store.payload(self.store.data[instruction as usize].range()).map_err(|_| problem("generic_call_payload"))?;
        let block_index = if self.store.tags[instruction as usize] == FullTag::ExprSelfCall { 0 } else { 1 };
        let block = words.get(block_index).and_then(|&id| IrBlockId::from_raw(id)).and_then(|id| self.store.blocks.get(id.index()))
            .ok_or_else(|| problem("generic_call_arguments"))?;
        let words = self.store.payload(block.instructions).map_err(|_| problem("generic_call_arguments"))?;
        let encoded = words.first().copied().ok_or_else(|| problem("generic_call_argument_arity"))? as usize;
        if encoded > count || words.len() != 1 + encoded * 2 { return Err(problem("generic_call_argument_arity")); }
        let scope = self.generic.as_ref().unwrap().scope(target).map_err(|_| problem("generic_call_scope"))?;
        let mut sources = words[1..].chunks_exact(2).enumerate().map(|(slot, argument)| match argument[0] {
            0 => Ok(Some(argument[1])),
            2 if argument[1] as usize == slot && scope.parameter_flags[slot] & 2 != 0 => Ok(None),
            _ => Err(problem("generic_call_splice_requires_prepared_binding")),
        }).collect::<Result<Vec<_>, _>>()?;
        for slot in encoded..count {
            if scope.parameter_flags[slot] & 2 == 0 { return Err(problem("generic_call_missing_required_argument")); }
            sources.push(None);
        }
        Ok(sources)
    }

    fn call_reference(&mut self, solved: &SolvedTypes, caller: Option<SchemeScopeId>, ty: GraphTypeId, substitution: bool) -> Result<TypeRef, IrBuildError> {
        if let Some(caller) = caller {
            let scheme = self.generic_schemes[&caller];
            if substitution {
                let ty = solved.graph.resolved(ty).map_err(|_| problem("generic_forwarding_substitution"))?;
                if let TypeNode::Rigid { .. } = solved.graph.node(ty).map_err(|_| problem("generic_forwarding_substitution"))? {
                    if let Some(index) = solved.graph.scheme_binder_index(scheme, ty).map_err(|_| problem("generic_forwarding_substitution"))? {
                        return Ok(TypeRef::Rigid(u32::try_from(index).map_err(|_| problem("generic_forwarding_quantifier"))?));
                    }
                }
            }
            self.reference(&solved.graph, scheme, ty)
        } else { self.ground(&solved.graph, ty).map(TypeRef::Ground) }
    }

    fn materialize(&mut self, reference: TypeRef, substitutions: &[TypeId]) -> Result<TypeId, IrBuildError> {
        self.generic.as_ref().ok_or_else(|| problem("generic_missing_builder"))?
            .materialize_reference(reference, substitutions, &mut self.store.semantic, &mut self.semantic)
            .map_err(|_| problem("generic_instantiation_type"))
    }

    fn reference_is_closed(&self, reference: TypeRef, depth: usize) -> Result<bool, IrBuildError> {
        if depth >= 512 { return Err(problem("generic_forwarding_type_depth")); }
        match reference {
            TypeRef::Ground(_) => Ok(true),
            TypeRef::Rigid(_) => Ok(false),
            TypeRef::Template(id) => {
                let template = self.generic.as_ref().unwrap().template(id).map_err(|_| problem("generic_forwarding_template"))?;
                match template {
                    TypeTemplate::Optional(inner) | TypeTemplate::List(inner) | TypeTemplate::Stream(inner) => self.reference_is_closed(*inner, depth + 1),
                    TypeTemplate::Map { key, value } => Ok(self.reference_is_closed(*key, depth + 1)? && self.reference_is_closed(*value, depth + 1)?),
                    TypeTemplate::Result { ok, error } => Ok(self.reference_is_closed(*ok, depth + 1)? && self.reference_is_closed(*error, depth + 1)?),
                    TypeTemplate::Record { fields, row_tail } => {
                        if row_tail.is_some() { return Ok(false); }
                        for &(_, ty) in fields { if !self.reference_is_closed(ty, depth + 1)? { return Ok(false); } }
                        Ok(true)
                    }
                    TypeTemplate::Arrow { parameters, result, .. } => {
                        for parameter in parameters { if !self.reference_is_closed(parameter.ty, depth + 1)? { return Ok(false); } }
                        self.reference_is_closed(*result, depth + 1)
                    }
                }
            }
        }
    }

    fn compose_reference(&mut self, reference: TypeRef, substitutions: &[TypeRef], cache: &mut FxHashMap<TypeRef, TypeRef>, depth: usize) -> Result<TypeRef, IrBuildError> {
        if depth >= 512 { return Err(problem("generic_forwarding_type_depth")); }
        if let Some(&result) = cache.get(&reference) { return Ok(result); }
        let result = match reference {
            TypeRef::Ground(_) => reference,
            TypeRef::Rigid(index) => *substitutions.get(index as usize).ok_or_else(|| problem("generic_forwarding_quantifier"))?,
            TypeRef::Template(id) => {
                let template = self.generic.as_ref().unwrap().template(id).map_err(|_| problem("generic_forwarding_template"))?.clone();
                let composed = match template {
                    TypeTemplate::Optional(inner) => TypeTemplate::Optional(self.compose_reference(inner, substitutions, cache, depth + 1)?),
                    TypeTemplate::List(inner) => TypeTemplate::List(self.compose_reference(inner, substitutions, cache, depth + 1)?),
                    TypeTemplate::Stream(inner) => TypeTemplate::Stream(self.compose_reference(inner, substitutions, cache, depth + 1)?),
                    TypeTemplate::Map { key, value } => TypeTemplate::Map { key: self.compose_reference(key, substitutions, cache, depth + 1)?, value: self.compose_reference(value, substitutions, cache, depth + 1)? },
                    TypeTemplate::Result { ok, error } => TypeTemplate::Result { ok: self.compose_reference(ok, substitutions, cache, depth + 1)?, error: self.compose_reference(error, substitutions, cache, depth + 1)? },
                    TypeTemplate::Record { fields, row_tail } => {
                        let mut fields = fields.iter().map(|&(name, ty)| self.compose_reference(ty, substitutions, cache, depth + 1).map(|ty| (name, ty))).collect::<Result<Vec<_>, _>>()?;
                        let row_tail = match row_tail {
                            None => None,
                            Some(index) => match substitutions.get(index as usize).copied().ok_or_else(|| problem("generic_forwarding_row_quantifier"))? {
                                TypeRef::Rigid(index) => Some(index),
                                TypeRef::Ground(ty) => {
                                    let (names, types) = self.store.semantic.record_fields(ty).map_err(|_| problem("generic_forwarding_row_not_record"))?;
                                    for (&name, &ty) in names.iter().zip(types) {
                                        fields.push((name, TypeRef::Ground(TypeId::from_raw(ty).ok_or_else(|| problem("generic_forwarding_row_field_type"))?)));
                                    }
                                    None
                                }
                                TypeRef::Template(id) => {
                                    let TypeTemplate::Record { fields: tail_fields, row_tail } = self.generic.as_ref().unwrap().template(id).map_err(|_| problem("generic_forwarding_row_template"))? else { return Err(problem("generic_forwarding_row_not_record")); };
                                    fields.extend(tail_fields.iter().copied());
                                    *row_tail
                                }
                            },
                        };
                        TypeTemplate::Record { fields: fields.into_boxed_slice(), row_tail }
                    }
                    TypeTemplate::Arrow { kind, parameters, result, effects } => {
                        let parameters = parameters.into_vec().into_iter().map(|mut parameter| {
                            parameter.ty = self.compose_reference(parameter.ty, substitutions, cache, depth + 1)?;
                            Ok(parameter)
                        }).collect::<Result<Vec<_>, IrBuildError>>()?;
                        TypeTemplate::Arrow { kind, parameters: parameters.into_boxed_slice(), result: self.compose_reference(result, substitutions, cache, depth + 1)?, effects }
                    }
                };
                let id = self.generic_evidence_mut().add_template(composed).map_err(|_| problem("generic_forwarding_template_allocation"))?;
                let reference = TypeRef::Template(id);
                // Closed references share canonical ground identities with ordinary signatures.
                if self.reference_is_closed(reference, 0)? { TypeRef::Ground(self.materialize(reference, &[])?) } else { reference }
            }
        };
        cache.insert(reference, result);
        Ok(result)
    }

    fn prepare_forwarded_requirements(&mut self, call: &PreparedCall, layouts: &mut BTreeMap<TypeId, PhysicalLayoutId>) -> Result<Vec<ForwardedRequirement>, IrBuildError> {
        let caller = self.generic.as_ref().unwrap().scope(call.caller.ok_or_else(|| problem("generic_forwarding_scope"))?).map_err(|_| problem("generic_forwarding_scope"))?.clone();
        let callee = self.generic.as_ref().unwrap().scope(call.target).map_err(|_| problem("generic_forwarding_scope"))?.clone();
        let sources = self.encoded_call_sources(call.instruction, callee.parameters.len(), call.target)?;
        let mut cache = FxHashMap::default();
        let mut mapping = Vec::with_capacity(callee.requirements.len());
        for requirement in &callee.requirements {
            let rebased = match *requirement {
                Requirement::Add { left, right, result } => Requirement::Add {
                    left: self.compose_reference(left, &call.substitutions, &mut cache, 0)?,
                    right: self.compose_reference(right, &call.substitutions, &mut cache, 0)?,
                    result: self.compose_reference(result, &call.substitutions, &mut cache, 0)?,
                },
                Requirement::Projection { receiver_parameter, field, result, .. } => {
                    let receiver = *call.parameters.get(receiver_parameter as usize).ok_or_else(|| problem("generic_forwarding_projection_parameter"))?;
                    let source = sources.get(receiver_parameter as usize).copied().flatten().ok_or_else(|| problem("generic_forwarding_projection_source"))?;
                    let receiver_parameter = if self.store.tags[source as usize] == FullTag::ExprParam {
                        *self.store.payload(self.store.data[source as usize].range()).map_err(|_| problem("generic_forwarding_projection_source"))?.first().ok_or_else(|| problem("generic_forwarding_projection_source"))?
                    } else if matches!(receiver, TypeRef::Ground(_)) { u32::MAX }
                    else { return Err(problem("generic_forwarding_projection_requires_parameter")); };
                    Requirement::Projection { receiver, receiver_parameter, field, result: self.compose_reference(result, &call.substitutions, &mut cache, 0)? }
                }
            };
            if let Some(index) = caller.requirements.iter().position(|candidate| *candidate == rebased) {
                mapping.push(ForwardedRequirement::Caller(index as u32));
                continue;
            }
            let witness = match rebased {
                Requirement::Projection { receiver: TypeRef::Ground(receiver), field, result: TypeRef::Ground(result), .. } => {
                    let layout = self.layout(receiver, layouts)?;
                    let physical = self.generic.as_ref().unwrap().layout(layout).map_err(|_| problem("generic_forwarding_projection_layout"))?;
                    let slot = physical.fields.iter().position(|entry| entry.0 == field).ok_or_else(|| problem("generic_forwarding_projection_missing_field"))?;
                    if physical.fields[slot].1 != result { return Err(problem("generic_forwarding_projection_field_type")); }
                    RequirementWitness::Projection { layout, field_slot: slot as u32, result }
                }
                Requirement::Add { left: TypeRef::Ground(left), right: TypeRef::Ground(right), result: TypeRef::Ground(result) } => {
                    let operation = match (self.store.semantic.to_type(left), self.store.semantic.to_type(right), self.store.semantic.to_type(result)) {
                        (Ok(Type::Int | Type::UInt), Ok(Type::Int | Type::UInt), Ok(Type::Int)) => ConcreteOperationId::AddInt,
                        (Ok(Type::Float), Ok(Type::Float), Ok(Type::Float)) => ConcreteOperationId::AddFloat,
                        (Ok(Type::Str), Ok(Type::Str), Ok(Type::Str)) => ConcreteOperationId::AddStr,
                        _ => return Err(problem("generic_forwarding_add_unsupported_domain")),
                    };
                    RequirementWitness::Add { operation, left, right, result }
                }
                _ => return Err(problem("generic_forwarding_requires_declaration_obligation")),
            };
            mapping.push(ForwardedRequirement::Fixed(witness));
        }
        Ok(mapping)
    }

    fn instantiate_prepared_call(&mut self, call: &PreparedCall, caller_substitutions: &[TypeId], layouts: &mut BTreeMap<TypeId, PhysicalLayoutId>) -> Result<InstantiationId, IrBuildError> {
        let substitutions = call.substitutions.iter().map(|&reference| self.materialize(reference, caller_substitutions)).collect::<Result<Vec<_>, _>>()?;
        let parameters = call.parameters.iter().map(|&reference| self.materialize(reference, caller_substitutions)).collect::<Result<Vec<_>, _>>()?;
        let result_type = self.materialize(call.result, caller_substitutions)?;
        let scope = self.generic.as_ref().unwrap().scope(call.target).map_err(|_| problem("generic_call_scope"))?.clone();
        if substitutions.len() != scope.quantifiers.len() || parameters.len() != scope.parameters.len() { return Err(problem("generic_instantiation_arity")); }
        let mut requirements = Vec::with_capacity(scope.requirements.len());
        for requirement in &scope.requirements {
            requirements.push(match *requirement {
                Requirement::Projection { receiver_parameter, field, result, .. } => {
                    let receiver = *parameters.get(receiver_parameter as usize).ok_or_else(|| problem("generic_projection_parameter"))?;
                    let layout = self.layout(receiver, layouts)?;
                    let physical = self.generic.as_ref().unwrap().layout(layout).map_err(|_| problem("generic_projection_layout"))?;
                    let slot = physical.fields.iter().position(|entry| entry.0 == field).ok_or_else(|| problem("generic_projection_missing_field"))?;
                    let actual_result = physical.fields[slot].1;
                    if actual_result != self.materialize(result, &substitutions)? { return Err(problem("generic_projection_field_type")); }
                    RequirementWitness::Projection { layout, field_slot: slot as u32, result: actual_result }
                }
                Requirement::Add { left, right, result } => {
                    let left = self.materialize(left, &substitutions)?;
                    let right = self.materialize(right, &substitutions)?;
                    let result = self.materialize(result, &substitutions)?;
                    let left_type = self.store.semantic.to_type(left).map_err(|_| problem("generic_add_operand"))?;
                    let right_type = self.store.semantic.to_type(right).map_err(|_| problem("generic_add_operand"))?;
                    let result_type = self.store.semantic.to_type(result).map_err(|_| problem("generic_add_result"))?;
                    let operation = match (&left_type, &right_type, &result_type) {
                        (Type::Int | Type::UInt, Type::Int | Type::UInt, Type::Int) => ConcreteOperationId::AddInt,
                        (Type::Float, Type::Float, Type::Float) => ConcreteOperationId::AddFloat,
                        (Type::Str, Type::Str, Type::Str) => ConcreteOperationId::AddStr,
                        _ => return Err(problem("generic_add_unsupported_domain")),
                    };
                    RequirementWitness::Add { operation, left, right, result }
                }
            });
        }
        self.generic_evidence_mut().add_instance(Instantiation { scope: call.target, substitutions: substitutions.into_boxed_slice(), parameter_types: parameters.into_boxed_slice(), result_type, requirements: requirements.into_boxed_slice() })
            .map_err(|_| problem("generic_instance_allocation"))
    }

    fn prepare_generic_calls(&mut self, solved: &SolvedTypes, layouts: &mut BTreeMap<TypeId, PhysicalLayoutId>) -> Result<(), IrBuildError> {
        let mut calls = Vec::new();
        for (instruction, expression, owner) in self.generic_expression_rows.clone() {
            let Some(call) = solved.calls.get(&expression) else { continue; };
            let Some(target) = call.declaration.and_then(|declaration| self.generic_declarations.get(&declaration).copied()) else { continue; };
            if !matches!(self.store.tags[instruction as usize], FullTag::ExprCall | FullTag::ExprDirectPureCall | FullTag::ExprSelfCall | FullTag::ExprDynamicCall) { return Err(problem("generic_call_instruction_origin")); }
            let caller = call.caller.and_then(|declaration| self.generic_declarations.get(&declaration).copied());
            let signature = solved.graph.resolved(call.signature).map_err(|_| problem("generic_call_signature"))?;
            let TypeNode::Arrow(signature) = solved.graph.node(signature).map_err(|_| problem("generic_call_signature"))? else { return Err(problem("generic_call_not_callable")); };
            let mut parameters = signature.params.iter().map(|parameter| self.call_reference(solved, caller, parameter.ty, false)).collect::<Result<Vec<_>, _>>()?;
            if call.binding.supplied_slots.len() != call.actual_arguments.len() { return Err(problem("generic_call_binding")); }
            for (&slot, &actual) in call.binding.supplied_slots.iter().zip(&call.actual_arguments) {
                let destination = parameters.get_mut(slot).ok_or_else(|| problem("generic_call_binding"))?;
                *destination = self.call_reference(solved, caller, actual, false)?;
            }
            let sources = self.encoded_call_sources(instruction, parameters.len(), target)?;
            let substitutions = call.substitutions.iter().map(|&ty| self.call_reference(solved, caller, ty, true)).collect::<Result<Vec<_>, _>>()?;
            let result = self.call_reference(solved, caller, signature.result, false)?;
            for (parameter, (&ty, &source_instruction)) in parameters.iter().zip(&sources).enumerate() {
                self.generic_evidence_mut().add_argument(SolvedArgument { call_instruction: instruction, parameter: parameter as u32, source_instruction, ty });
            }
            calls.push(PreparedCall { instruction, owner, target, caller, substitutions, parameters, result });
        }
        let mut pending = Vec::new();
        let mut by_caller: FxHashMap<SchemeScopeId, Vec<usize>> = FxHashMap::default();
        let mut queue = std::collections::VecDeque::new();
        let mut visited = rustc_hash::FxHashSet::default();
        for call in calls {
            if let Some(caller) = call.caller {
                by_caller.entry(caller).or_default().push(pending.len());
                let requirements = self.prepare_forwarded_requirements(&call, layouts)?;
                pending.push(PreparedForwarding { call, requirements, edges: Vec::new() });
            } else {
                let instance = self.instantiate_prepared_call(&call, &[], layouts)?;
                self.generic_evidence_mut().add_call(SolvedCall { instruction: call.instruction, caller: call.owner, target: call.target, evidence: CallEvidence::Ground(instance) });
                if visited.insert(instance) { queue.push_back(instance); }
            }
        }
        while let Some(instance) = queue.pop_front() {
            let caller = self.generic.as_ref().unwrap().instance(instance).map_err(|_| problem("generic_forwarding_instance"))?.clone();
            for &index in by_caller.get(&caller.scope).map(Vec::as_slice).unwrap_or(&[]) {
                let edge = &mut pending[index];
                let destination = self.instantiate_prepared_call(&edge.call, &caller.substitutions, layouts)?;
                let callee = self.generic.as_ref().unwrap().instance(destination).map_err(|_| problem("generic_forwarding_instance"))?;
                for (&mapping, actual) in edge.requirements.iter().zip(&callee.requirements) {
                    let expected = match mapping { ForwardedRequirement::Caller(index) => caller.requirements[index as usize], ForwardedRequirement::Fixed(witness) => witness };
                    if expected != *actual { return Err(problem("generic_forwarding_requirement_relationship")); }
                }
                edge.edges.push((instance, destination));
                if visited.insert(destination) { queue.push_back(destination); }
                if visited.len() > 2_000_000 { return Err(problem("generic_instantiation_limit")); }
            }
        }
        for edge in pending {
            let caller = edge.call.caller.ok_or_else(|| problem("generic_forwarding_scope"))?;
            let requirements = edge.requirements;
            let plan = self.generic_evidence_mut().add_forwarding(ForwardingPlan { caller, callee: edge.call.target, substitutions: edge.call.substitutions.into_boxed_slice(), requirements: requirements.into_boxed_slice(), instances: edge.edges.into_boxed_slice() })
                .map_err(|_| problem("generic_forwarding_allocation"))?;
            self.generic_evidence_mut().add_call(SolvedCall { instruction: edge.call.instruction, caller: edge.call.owner, target: edge.call.target, evidence: CallEvidence::Forwarded(plan) });
        }
        Ok(())
    }
}
