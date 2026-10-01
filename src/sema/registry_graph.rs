use crate::modules::signature::{ApiArgCheck, ImplBinding, MethodReceiver, ModuleFnSig, SemanticRule, api_spec};
use crate::modules::RuntimeOp;
use crate::sema::inference::{ArgumentRelation, Arrow, CallableKind, CandidateId, CandidateTemplate, EffectRole, EffectRoleReference, EffectSet, EffectSummary, Eligibility, Generalization, GraphOwner, InferenceContext, InferenceError, ModuleField, OperationFamilyId, Parameter, ProducerRole, RequirementId, RowField, SchemeId, TypeId};
use crate::sema::types::{ModuleExportType, Type};
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::node::Effect;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use xsh_registry::types::BuiltinTypeParameter;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryOwner {
    Module(&'static str),
    Method(MethodReceiver),
    TypeConstructor(&'static str),
}

#[derive(Clone, Debug)]
pub(crate) struct RegistryParameter {
    pub label: Name,
    pub defaulted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryLifecycle {
    None,
    CollectReceiver { materialized: bool },
    ResultProducer { pull: EffectSet, close: EffectSet },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryProducerInput { Receiver, Argument(u32) }

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RegistryProducerComponent { ListItem, MapKey, MapValue, OptionalPayload, ResultSuccess, ResultError, RecordField(Name) }

/// Paths retain possible producer ownership, independently of type equality.
/// A collection selector can choose any item in its homogeneous input profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RegistryProducerTransfer {
    pub input: RegistryProducerInput,
    pub input_path: Box<[RegistryProducerComponent]>,
    pub output_path: Box<[RegistryProducerComponent]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RegistryProducerTransferPlan {
    Empty,
    Transfers(Box<[RegistryProducerTransfer]>),
    Opaque,
}

/// Native producer permissions describe the work done during a pull, even when
/// creation has already acquired a file or enumerated its initial inputs.
fn registry_lifecycle(owner: RegistryOwner, operation: RuntimeOp) -> RegistryLifecycle {
    use RuntimeOp::*;
    let pull=match operation {
        FsStreamLines|FsBytesLines|ArchiveCpioList|FsChildren|FsDirs|FsFiles|FsMounts|FsWalk=>EffectSet::FS,
        LinuxBlockDevices|LinuxDiskUsage|LinuxInterfaces|LinuxLoopList|LinuxOpenFiles|LinuxRfkillList|LinuxUeventStream|ProcessList|ProcessThreads=>EffectSet::PROCESS,
        ArchiveTarList|ArchiveZipList|LinuxModules|LinuxDmesg|LinuxRoutes|ProcessPort|ProcessPorts|ProcessPortsForPid|UnixReapChildEvents=>EffectSet::EMPTY,
        StreamCollect=>return RegistryLifecycle::CollectReceiver { materialized: owner==RegistryOwner::Method(MethodReceiver::List) },
        _=>return RegistryLifecycle::None,
    };
    RegistryLifecycle::ResultProducer { pull,close:EffectSet::EMPTY }
}

/// Canonical registry metadata accompanies the graph candidate without owning
/// another inferred signature. Source binding and descriptor facts remain separate.
#[derive(Clone, Debug)]
pub(crate) struct RegistryCandidate {
    pub identity: Name,
    pub public_label: Name,
    pub scheme: SchemeId,
    pub owner: RegistryOwner,
    pub entry: &'static str,
    pub operation: RuntimeOp,
    pub binding: ImplBinding,
    pub kind: CallableKind,
    pub command: bool,
    pub argument_check: ApiArgCheck,
    pub semantic_rule: SemanticRule,
    pub parameters: Box<[RegistryParameter]>,
    pub required_effect: Option<Effect>,
    pub lifecycle: RegistryLifecycle,
    pub producer_transfer: RegistryProducerTransferPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryReferenceBoundary { Receiver, InputEffects, ArgumentCheck, SemanticRule }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryReferenceProtocol { Arrow, Native, Boundary(RegistryReferenceBoundary) }

impl RegistryCandidate {
    pub(crate) fn reference_family_member_supported(&self, template: &CandidateTemplate) -> bool {
        !template.has_receiver && template.effect_roles.is_empty()
            && self.semantic_rule == SemanticRule::Standard
            && matches!(self.argument_check, ApiArgCheck::Standard | ApiArgCheck::JsonCompatible | ApiArgCheck::CommandArgv)
    }

    /// References retain invocation guards and producer transfers. Argument
    /// syntax and descriptor preparation need their own original source plans.
    pub(crate) fn reference_protocol(&self, template: &CandidateTemplate) -> RegistryReferenceProtocol {
        use RegistryReferenceBoundary as Boundary;
        if template.has_receiver { return RegistryReferenceProtocol::Boundary(Boundary::Receiver); }
        if !template.effect_roles.is_empty() { return RegistryReferenceProtocol::Boundary(Boundary::InputEffects); }
        if self.semantic_rule != SemanticRule::Standard { return RegistryReferenceProtocol::Boundary(Boundary::SemanticRule); }
        if !matches!(self.argument_check, ApiArgCheck::Standard | ApiArgCheck::JsonCompatible) {
            return RegistryReferenceProtocol::Boundary(Boundary::ArgumentCheck);
        }
        if !template.actual_eligibility.is_empty() || !template.output_effect_roles.is_empty()
            || !matches!(self.producer_transfer, RegistryProducerTransferPlan::Empty) {
            RegistryReferenceProtocol::Native
        } else { RegistryReferenceProtocol::Arrow }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.parameters.len() * std::mem::size_of::<RegistryParameter>() + match &self.producer_transfer {
            RegistryProducerTransferPlan::Transfers(transfers) => transfers.len() * std::mem::size_of::<RegistryProducerTransfer>()
                + transfers.iter().map(|transfer| (transfer.input_path.len() + transfer.output_path.len()) * std::mem::size_of::<RegistryProducerComponent>()).sum::<usize>(),
            RegistryProducerTransferPlan::Empty | RegistryProducerTransferPlan::Opaque => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryProducerPath { ResultSuccess }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RegistryProducerAlternative {
    pub candidate:CandidateId,
    pub native_path:Option<RegistryProducerPath>,
}

#[derive(Clone, Debug)]
pub(crate) struct RegistryProducerRelation {
    pub alternatives:Box<[RegistryProducerAlternative]>,
    pub output_roles:Box<[ProducerRole]>,
}

#[derive(Clone, Debug)]
pub(crate) struct RegistrySchema {
    pub authority_id: Name,
    pub label: Name,
    pub shape: TypeId,
}

#[derive(Clone, Debug)]
pub(crate) struct RegistryErrorField { pub label:Name, pub ty:TypeId }

#[derive(Clone, Debug)]
pub(crate) struct RegistryErrorVariant {
    pub authority_id:Name,
    pub family:Name,
    pub variant:Name,
    pub result:TypeId,
    pub parameters:Box<[RegistryErrorField]>,
    pub facets:Box<[Name]>,
}

/// A catalog belongs to one checked bundle. Source-check rollback can retire
/// cached handles; the next lookup imports their canonical definitions again.
#[derive(Default)]
pub(crate) struct RegistryGraph {
    owner: Option<GraphOwner>,
    families: BTreeMap<String, OperationFamilyId>,
    candidates: FxHashMap<CandidateId, RegistryCandidate>,
    schemas:BTreeMap<String,RegistrySchema>,
    errors:BTreeMap<String,RegistryErrorVariant>,
}

impl RegistryGraph {
    fn bind_owner(&mut self, graph: &InferenceContext) -> Result<(), InferenceError> {
        if self.owner.is_some_and(|owner| owner != graph.owner()) { return Err(InferenceError::ForeignHandle); }
        self.owner = Some(graph.owner());
        Ok(())
    }

    fn cached_family(&mut self, graph: &InferenceContext, key: &str) -> Option<OperationFamilyId> {
        let id = *self.families.get(key)?;
        if graph.family(id).is_ok() { return Some(id); }
        self.families.retain(|_, id| graph.family(*id).is_ok());
        self.candidates.retain(|id, _| graph.candidate(*id).is_ok());
        None
    }

    /// A schema shape is retained for explicit validation. Importing it does
    /// not certify an erased value as an instance of that shape.
    pub(crate) fn schema(&mut self,graph:&mut InferenceContext,name:&str,origin:Span)->Result<RegistrySchema,InferenceError> {
        self.bind_owner(graph)?;
        if let Some(schema)=self.schemas.get(name) { if graph.node(schema.shape).is_ok() { return Ok(schema.clone()); } }
        let ty=xsh_registry::records::standard_record_type(name).ok_or(InferenceError::Boundary("unknown registry schema"))?;
        let ty=crate::modules::signature::convert_type(&ty);
        let schema=graph.probe(|graph| { let reason=graph.reason(origin,None)?; let shape=import_type(graph,&ty,&mut BTreeMap::new(),&mut Vec::new(),reason,origin,0)?; graph.solve()?;
            Ok(RegistrySchema { authority_id:Name::intern(&format!("registry.schema.{name}")),label:Name::intern(name),shape }) })?;
        self.schemas.insert(name.to_owned(),schema.clone()); Ok(schema)
    }

    /// Builtin variants retain their declaring family and facets. Importing
    /// their payload contract grants no source-level construction permission.
    pub(crate) fn builtin_error_variant(&mut self,graph:&mut InferenceContext,family:&str,variant:&str,origin:Span)->Result<RegistryErrorVariant,InferenceError> {
        self.bind_owner(graph)?; let key=format!("{family}.{variant}");
        if let Some(error)=self.errors.get(&key) { if graph.node(error.result).is_ok()&&error.parameters.iter().all(|field|graph.node(field.ty).is_ok()) { return Ok(error.clone()); } }
        let descriptor=xsh_registry::errors::builtin_error_families().into_iter().find(|descriptor|descriptor.name==family).ok_or(InferenceError::Boundary("unknown registry error family"))?;
        let variant_descriptor=descriptor.variants.iter().find(|descriptor|descriptor.name==variant).ok_or(InferenceError::Boundary("unknown registry error variant"))?;
        let error=graph.probe(|graph| { let reason=graph.reason(origin,None)?;
            let parameters=descriptor.fields.iter().map(|field|Ok(RegistryErrorField { label:Name::intern(field.name),ty:import_type(graph,&crate::modules::signature::convert_type(&field.ty),&mut BTreeMap::new(),&mut Vec::new(),reason,origin,0)? })).collect::<Result<Vec<_>,InferenceError>>()?;
            let family=Name::intern(family); let variant=Name::intern(variant);
            let result=graph.import_type(&Type::ErrorVariant { family,variant },0,origin)?; graph.solve()?;
            Ok(RegistryErrorVariant { authority_id:Name::intern(&format!("registry.error.{key}")),family,variant,result,parameters:parameters.into(),facets:variant_descriptor.facets.iter().map(|facet|Name::intern(facet)).collect() }) })?;
        self.errors.insert(key,error.clone()); Ok(error)
    }

    pub(crate) fn module_family(&mut self, graph: &mut InferenceContext, module: &str, entry: &str, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.bind_owner(graph)?;
        let key = format!("module.{module}.{entry}");
        if let Some(id) = self.cached_family(graph, &key) { return Ok(id); }
        let (module, signatures) = api_spec().module_entries().find(|(name, _)| *name == module).ok_or(InferenceError::Boundary("unknown registry module"))?;
        let entry = signatures.functions.iter().find(|function| function.name == entry).ok_or(InferenceError::Boundary("unknown registry callable"))?;
        self.build_family(graph, key, |catalog, graph| entry.overloads.iter().map(|signature| catalog.import_candidate(graph, RegistryOwner::Module(module), entry.name, signature, None, origin)).collect())
    }

    pub(crate) fn method_family(&mut self, graph: &mut InferenceContext, entry: &str, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.bind_owner(graph)?;
        let key = format!("method.{entry}");
        if let Some(id) = self.cached_family(graph, &key) { return Ok(id); }
        self.build_family(graph, key, |catalog, graph| {
            let mut candidates = Vec::new();
            for (receiver, methods) in api_spec().method_entries() {
                if receiver == MethodReceiver::PathConstructor { continue; }
                for method in methods.iter().filter(|method| method.name == entry) {
                    for signature in &method.overloads {
                        let receiver_type = receiver_type(receiver, signature.receiver_ty.as_ref())?;
                        candidates.push(catalog.import_candidate(graph, RegistryOwner::Method(receiver), method.name, &signature.sig, Some(&receiver_type), origin)?);
                    }
                }
            }
            if candidates.is_empty() { return Err(InferenceError::Boundary("unknown registry method")); }
            Ok(candidates)
        })
    }

    pub(crate) fn type_constructor_family(&mut self, graph: &mut InferenceContext, owner: &str, entry: &str, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        self.bind_owner(graph)?;
        let key = format!("constructor.{owner}.{entry}");
        if let Some(id) = self.cached_family(graph, &key) { return Ok(id); }
        let (receiver, methods) = api_spec().method_entries().find(|(receiver, _)| *receiver == MethodReceiver::PathConstructor && xsh_registry::signature::receiver_name(*receiver) == owner).ok_or(InferenceError::Boundary("unknown registry type constructor owner"))?;
        let entry = methods.iter().find(|method| method.name == entry).ok_or(InferenceError::Boundary("unknown registry type constructor"))?;
        let owner = RegistryOwner::TypeConstructor(xsh_registry::signature::receiver_name(receiver));
        self.build_family(graph, key, |catalog, graph| entry.overloads.iter().map(|signature| catalog.import_candidate(graph, owner, entry.name, &signature.sig, None, origin)).collect())
    }

    fn build_family(&mut self, graph: &mut InferenceContext, key: String, import: impl FnOnce(&mut Self, &mut InferenceContext) -> Result<Vec<CandidateId>, InferenceError>) -> Result<OperationFamilyId, InferenceError> {
        let result = graph.probe(|graph| {
            let candidates = import(self, graph)?;
            self.finish_family(graph, key, &candidates)
        });
        if result.is_err() {
            self.families.retain(|_, id| graph.family(*id).is_ok());
            self.candidates.retain(|id, _| graph.candidate(*id).is_ok());
        }
        result
    }

    fn finish_family(&mut self, graph: &mut InferenceContext, key: String, candidates: &[CandidateId]) -> Result<OperationFamilyId, InferenceError> {
        let family = graph.register_family(candidates)?;
        self.families.insert(key, family);
        Ok(family)
    }

    pub(crate) fn metadata(&self, graph: &InferenceContext, id: CandidateId) -> Result<&RegistryCandidate, InferenceError> {
        if self.owner != Some(graph.owner()) { return Err(InferenceError::ForeignHandle); }
        let template = graph.candidate(id)?;
        let metadata = self.candidates.get(&id).ok_or(InferenceError::ForeignHandle)?;
        if template.identity != metadata.identity || template.public_label != metadata.public_label || template.scheme != metadata.scheme || template.has_receiver != matches!(metadata.owner, RegistryOwner::Method(_)) { return Err(InferenceError::ForeignHandle); }
        Ok(metadata)
    }

    pub(crate) fn retained_candidates(&self, graph: &InferenceContext, candidates: &[CandidateId]) -> Result<Vec<(CandidateId, RegistryCandidate)>, InferenceError> {
        if self.owner.is_some_and(|owner| owner != graph.owner()) { return Err(InferenceError::ForeignHandle); }
        let mut retained = Vec::new();
        for &candidate in candidates {
            graph.candidate(candidate)?;
            if self.candidates.contains_key(&candidate) { retained.push((candidate, self.metadata(graph, candidate)?.clone())); }
        }
        Ok(retained)
    }

    pub(crate) fn retained_schemas(&self, graph: &InferenceContext) -> Result<Vec<RegistrySchema>, InferenceError> {
        if self.owner.is_some_and(|owner| owner != graph.owner()) { return Err(InferenceError::ForeignHandle); }
        let mut retained = Vec::new();
        for schema in self.schemas.values() {
            if graph.node(schema.shape).is_err() { continue; }
            if self.owner != Some(graph.owner()) { return Err(InferenceError::ForeignHandle); }
            retained.push(schema.clone());
        }
        Ok(retained)
    }

    pub(crate) fn retained_errors(&self, graph: &InferenceContext) -> Result<Vec<RegistryErrorVariant>, InferenceError> {
        if self.owner.is_some_and(|owner| owner != graph.owner()) { return Err(InferenceError::ForeignHandle); }
        let mut retained = Vec::new();
        for error in self.errors.values() {
            if graph.node(error.result).is_err() { continue; }
            if self.owner != Some(graph.owner()) { return Err(InferenceError::ForeignHandle); }
            for parameter in &error.parameters { graph.node(parameter.ty)?; }
            retained.push(error.clone());
        }
        Ok(retained)
    }

    pub(crate) fn producer_result_transfer(&self, graph: &InferenceContext, candidate: CandidateId) -> Result<&RegistryProducerTransferPlan, InferenceError> {
        Ok(&self.metadata(graph, candidate)?.producer_transfer)
    }

    /// The alternatives describe newly created native producers. Relationships
    /// that transfer producers from receiver or argument values remain separate.
    pub(crate) fn producer_result_relation(&self,graph:&InferenceContext,family:OperationFamilyId)->Result<Option<RegistryProducerRelation>,InferenceError> {
        let candidates=graph.family(family)?; let mut alternatives=Vec::with_capacity(candidates.len()); let mut has_native=false; let mut output_roles=None;
        for candidate in candidates {
            let metadata=self.metadata(graph,*candidate)?; let native_path=if matches!(metadata.lifecycle,RegistryLifecycle::ResultProducer { .. }) { has_native=true; Some(RegistryProducerPath::ResultSuccess) } else { None };
            alternatives.push(RegistryProducerAlternative { candidate:*candidate,native_path });
            let roles:Vec<_>=graph.candidate(*candidate)?.output_effect_roles.iter().map(|(role,_)|*role).collect();
            if output_roles.as_ref().is_some_and(|prior|*prior!=roles) { return Err(InferenceError::InvalidScheme); } output_roles=Some(roles);
        }
        if !has_native { return Ok(None); }
        let roles=output_roles.ok_or(InferenceError::InvalidScheme)?; if roles!=[ProducerRole::Pull,ProducerRole::Close] { return Err(InferenceError::InvalidScheme); }
        Ok(Some(RegistryProducerRelation { alternatives:alternatives.into(),output_roles:roles.into() }))
    }

    pub(crate) fn output_effect_bindings(&self,graph:&mut InferenceContext,family:OperationFamilyId,level:u32)->Result<Vec<(ProducerRole,EffectSummary)>,InferenceError> {
        let candidates=graph.family(family)?.to_vec(); let first=*candidates.first().ok_or(InferenceError::InvalidScheme)?;
        self.metadata(graph,first)?; let roles:Vec<_>=graph.candidate(first)?.output_effect_roles.iter().map(|(role,_)|*role).collect();
        for candidate in candidates { self.metadata(graph,candidate)?; if graph.candidate(candidate)?.output_effect_roles.iter().map(|(role,_)|*role).collect::<Vec<_>>()!=roles { return Err(InferenceError::InvalidScheme); } }
        roles.into_iter().map(|role|Ok((role,EffectSummary::Variable(graph.fresh_derived_effect_at(level,None)?)))).collect()
    }

    fn import_candidate(&mut self, graph: &mut InferenceContext, owner: RegistryOwner, entry: &'static str, signature: &ModuleFnSig, receiver: Option<&Type>, origin: Span) -> Result<CandidateId, InferenceError> {
        let identity = Name::intern(&format!("registry:{}", canonical_signature(owner, entry, signature, receiver)?));
        let public_owner = match owner { RegistryOwner::Module(owner) | RegistryOwner::TypeConstructor(owner) => owner, RegistryOwner::Method(receiver) => xsh_registry::signature::receiver_name(receiver) };
        let public_label = Name::intern(&format!("{public_owner}.{entry}"));
        let reason = graph.reason(origin, None)?;
        let mut variables = BTreeMap::new();
        let mut requirements = Vec::new();
        let mut parameters = Vec::with_capacity(signature.params.len() + usize::from(receiver.is_some()));
        if let Some(receiver) = receiver {
            let receiver = import_type(graph, receiver, &mut variables, &mut requirements, reason, origin, 0)?;
            // Self denotes the entire receiver, including its container shape.
            variables.insert(BuiltinTypeParameter::Receiver, receiver);
            parameters.push(Parameter { label: Name::intern("<receiver>"), ty: receiver, defaulted: false, rest: false });
        }
        for parameter in &signature.params {
            let ty = import_type(graph, &parameter.ty, &mut variables, &mut requirements, reason, origin, 0)?;
            parameters.push(Parameter { label: Name::intern(parameter.name), ty, defaulted: parameter.defaulted, rest: false });
        }
        let result = import_type(graph, &signature.return_ty, &mut variables, &mut requirements, reason, origin, 0)?;
        let lifecycle=registry_lifecycle(owner,signature.op); let declared_effects=EffectSummary::Closed(effect_bits(signature.effect.as_ref()));
        let mut effect_roots=Vec::new(); let mut input_roles=Vec::new(); let mut output_effect_roles=Vec::new();
        let effects=if let RegistryLifecycle::CollectReceiver { materialized }=lifecycle {
            let upper=materialized.then_some(EffectSet::EMPTY);
            let pull=EffectSummary::Variable(graph.fresh_effect_at(1,upper)?); let close=EffectSummary::Variable(graph.fresh_effect_at(1,upper)?);
            let creation=EffectSummary::Variable(graph.fresh_derived_effect_at(1,None)?);
            for source in [declared_effects,pull,close] { graph.include_effects(source,creation,reason)?; }
            input_roles.extend([(EffectRole::Pull { source:0 },0usize),(EffectRole::Close { source:0 },1usize)]); effect_roots.extend([pull,close]); creation
        } else { declared_effects };
        if registry_family_has_producer(owner,entry) {
            let (pull,close)=if let RegistryLifecycle::ResultProducer { pull,close }=lifecycle { (pull,close) } else { (EffectSet::EMPTY,EffectSet::EMPTY) };
            for (role,effects) in [(ProducerRole::Pull,pull),(ProducerRole::Close,close)] { output_effect_roles.push((role,effect_roots.len() as u32)); effect_roots.push(EffectSummary::Closed(effects)); }
        }
        let arrow=graph.arrow(Arrow { kind:if signature.pure { CallableKind::Pure } else { CallableKind::Proc },params:parameters,result,effects })?;
        let scheme=graph.generalize_with_effect_roots(arrow,0,Generalization::Allowed,&requirements,&effect_roots)?;
        let effect_roles=input_roles.into_iter().map(|(role,root)|match graph.scheme(scheme)?.effect_roots.get(root) {
            Some(EffectSummary::Rigid { scope,index }) if *scope==scheme=>Ok((role,EffectRoleReference::Binder(*index))),
            Some(EffectSummary::Closed(bits))=>Ok((role,EffectRoleReference::Fixed(*bits))),_=>Err(InferenceError::InvalidScheme),
        }).collect::<Result<Vec<_>,_>>()?;
        let mut argument_relations: Vec<_> = receiver.into_iter().chain(signature.params.iter().map(|parameter| &parameter.ty)).map(|ty| if declared_erasure(ty) { ArgumentRelation::DeclaredErasure } else { ArgumentRelation::Assignable }).collect();
        if signature.arg_check == ApiArgCheck::CommandArgv {
            use crate::sema::inference::CommandTextDomain;
            let domain = |ty: &Type| match ty { Type::Str => Ok(CommandTextDomain::Str), Type::Path => Ok(CommandTextDomain::Path), _ => Err(InferenceError::InvalidScheme) };
            if receiver.is_some() || signature.op != RuntimeOp::ProcessCommandArgv { return Err(InferenceError::InvalidScheme); }
            let [target, argv, ..] = signature.params.as_slice() else { return Err(InferenceError::InvalidScheme); };
            let Type::List(item) = &argv.ty else { return Err(InferenceError::InvalidScheme); };
            argument_relations[0] = ArgumentRelation::CommandTarget { domain: domain(&target.ty)? };
            argument_relations[1] = ArgumentRelation::CommandArgv { element: domain(item)? };
        }
        let id = graph.register_candidate(CandidateTemplate { failure_projection: None, output_effect_roles, public_label, effect_roles, identity, scheme, has_receiver: receiver.is_some(), actual_eligibility: actual_eligibility(signature)?, argument_relations })?;
        self.candidates.insert(id, RegistryCandidate {
            identity, public_label, scheme, owner, entry, operation: signature.op, binding: signature.binding, command: signature.command,
            kind: if signature.pure { CallableKind::Pure } else { CallableKind::Proc },
            argument_check: signature.arg_check, semantic_rule: signature.semantic_rule,
            parameters: signature.params.iter().map(|parameter| RegistryParameter { label: Name::intern(parameter.name), defaulted: parameter.defaulted }).collect(),
            required_effect: signature.effect.clone(), lifecycle: registry_lifecycle(owner,signature.op),
            producer_transfer: producer_transfer(owner, signature),
        });
        Ok(id)
    }
}

fn registry_family_has_producer(owner:RegistryOwner,entry:&str)->bool {
    let produces=|owner,operation|matches!(registry_lifecycle(owner,operation),RegistryLifecycle::ResultProducer { .. });
    match owner {
        RegistryOwner::Module(module)=>api_spec().module(module).and_then(|module|module.functions.iter().find(|function|function.name==entry)).is_some_and(|function|function.overloads.iter().any(|signature|produces(owner,signature.op))),
        RegistryOwner::Method(_)=>api_spec().method_entries().filter(|(receiver,_)|*receiver!=MethodReceiver::PathConstructor).any(|(receiver,methods)|methods.iter().filter(|method|method.name==entry).any(|method|method.overloads.iter().any(|signature|produces(RegistryOwner::Method(receiver),signature.sig.op)))),
        RegistryOwner::TypeConstructor(_)=>false,
    }
}

fn producer_transfer(owner: RegistryOwner, signature: &ModuleFnSig) -> RegistryProducerTransferPlan {
    use RegistryProducerComponent::{ListItem, MapValue, ResultSuccess};
    use RegistryProducerInput::{Argument, Receiver};
    use RuntimeOp::*;
    let transfer = |input, source: &[RegistryProducerComponent], result: &[RegistryProducerComponent]| RegistryProducerTransfer {
        input, input_path: source.into(), output_path: result.into(),
    };
    let paths = match (owner, signature.op) {
        (RegistryOwner::Module(_), MapEmpty | SetEmpty) => return RegistryProducerTransferPlan::Empty,
        (RegistryOwner::Method(MethodReceiver::List), ListPush) => vec![transfer(Receiver, &[], &[]), transfer(Argument(0), &[], &[ListItem])],
        (RegistryOwner::Method(MethodReceiver::List), ListExtend) => vec![transfer(Receiver, &[], &[]), transfer(Argument(0), &[], &[])],
        (RegistryOwner::Method(MethodReceiver::List), ListGet) => vec![transfer(Receiver, &[ListItem], &[ResultSuccess])],
        (RegistryOwner::Method(MethodReceiver::List), StreamCollect) => vec![transfer(Receiver, &[], &[])],
        (RegistryOwner::Method(MethodReceiver::Map), MapGet) => vec![transfer(Receiver, &[MapValue], &[ResultSuccess])],
        (RegistryOwner::Method(MethodReceiver::Map), MapSet) => vec![transfer(Receiver, &[], &[]), transfer(Argument(1), &[], &[MapValue])],
        (RegistryOwner::Method(MethodReceiver::Map), MapPush) => vec![transfer(Receiver, &[], &[]), transfer(Argument(1), &[], &[MapValue, ListItem])],
        (RegistryOwner::Method(MethodReceiver::Map), MapRemove) => vec![transfer(Receiver, &[], &[])],
        (RegistryOwner::Method(MethodReceiver::Map), MapValues) => vec![transfer(Receiver, &[MapValue], &[ListItem])],
        (RegistryOwner::Method(MethodReceiver::Map), MapKeys) => return RegistryProducerTransferPlan::Empty,
        (RegistryOwner::Method(MethodReceiver::Result), ResultContext) => vec![transfer(Receiver, &[], &[])],
        _ => {
            let native_scalar_items = matches!(registry_lifecycle(owner, signature.op), RegistryLifecycle::ResultProducer { .. })
                && matches!(&signature.return_ty, Type::Result(success, _) if matches!(success.as_ref(), Type::Stream(item) if producer_free_result(item)));
            return if native_scalar_items || producer_free_result(&signature.return_ty) { RegistryProducerTransferPlan::Empty } else { RegistryProducerTransferPlan::Opaque };
        }
    };
    RegistryProducerTransferPlan::Transfers(paths.into())
}

fn producer_free_result(ty: &Type) -> bool {
    match ty {
        Type::BuiltinParameter(_) | Type::Any | Type::Unknown | Type::Invalid | Type::Inference(_) | Type::Graph(_)
        | Type::Stream(_) | Type::ErasedRecord | Type::Module(_) | Type::DynamicModule | Type::Pure | Type::Proc | Type::Command | Type::Tag(_) => false,
        Type::List(item) | Type::Optional(item) => producer_free_result(item),
        Type::Map(key, value) | Type::Result(key, value) => producer_free_result(key) && producer_free_result(value),
        Type::Record(fields) => fields.values().all(producer_free_result),
        _ => true,
    }
}

fn declared_erasure(ty: &Type) -> bool {
    match ty {
        Type::Any | Type::ErasedRecord => true,
        Type::List(item) | Type::Stream(item) | Type::Optional(item) => declared_erasure(item),
        Type::Map(key, value) | Type::Result(key, value) => declared_erasure(key) || declared_erasure(value),
        Type::Record(fields) => fields.values().any(declared_erasure),
        Type::Module(fields) => fields.values().any(|export| matches!(export, ModuleExportType::Value { ty, .. } if declared_erasure(ty))),
        _ => false,
    }
}

fn receiver_type(receiver: MethodReceiver, template: Option<&Type>) -> Result<Type, InferenceError> {
    if let Some(template) = template { return Ok(template.clone()); }
    Ok(match receiver {
        MethodReceiver::EnvPathList => Type::EnvPathList, MethodReceiver::Path => Type::Path,
        MethodReceiver::Int => Type::Int, MethodReceiver::Float => Type::Float,
        MethodReceiver::Record => Type::ErasedRecord, MethodReceiver::Str => Type::Str,
        MethodReceiver::Bytes => Type::Bytes, MethodReceiver::Status => Type::Status,
        MethodReceiver::Digest => Type::Digest, MethodReceiver::Regex => Type::Regex,
        MethodReceiver::ProcessHandle => Type::ProcessHandle, MethodReceiver::NetJob => Type::NetJob,
        MethodReceiver::FsRoot => Type::FsRoot,
        MethodReceiver::PathConstructor | MethodReceiver::List | MethodReceiver::Map | MethodReceiver::Stream | MethodReceiver::Result => return Err(InferenceError::Boundary("receiver relationship lacks its canonical template")),
    })
}

// Supplied types certify data eligibility. An erased formal such as List[Any]
// cannot certify the JSON shape of the value supplied to that parameter.
fn actual_eligibility(signature: &ModuleFnSig) -> Result<Vec<(usize, Eligibility)>, InferenceError> {
    if signature.arg_check == ApiArgCheck::CommandArgv {
        if signature.op != RuntimeOp::ProcessCommandArgv { return Err(InferenceError::InvalidScheme); }
        return Ok(vec![(0, Eligibility::CommandTarget), (1, Eligibility::CommandArgv)]);
    }
    if signature.arg_check != ApiArgCheck::JsonCompatible { return Ok(Vec::new()); }
    let slots: &[usize] = match signature.op {
        RuntimeOp::JsonEncode | RuntimeOp::JsonEncodeLines => &[0],
        RuntimeOp::JsonWrite | RuntimeOp::JsonWriteLines => &[1],
        RuntimeOp::JsonSet => &[0, 2],
        _ => return Err(InferenceError::Boundary("JSON argument contract lacks canonical operand selectors")),
    };
    if slots.iter().any(|slot| *slot >= signature.params.len()) { return Err(InferenceError::InvalidScheme); }
    Ok(slots.iter().map(|slot| (*slot, Eligibility::JsonCompatible)).collect())
}

fn effect_bits(effect: Option<&Effect>) -> EffectSet {
    match effect {
        None => EffectSet::EMPTY, Some(Effect::Fs) => EffectSet::FS, Some(Effect::Net) => EffectSet::NET,
        Some(Effect::Process) => EffectSet::PROCESS, Some(Effect::Env) => EffectSet::ENV,
        Some(Effect::Time) => EffectSet::TIME, Some(Effect::Error) => EffectSet::ERROR, Some(Effect::Io) => EffectSet::IO,
    }
}

fn import_type(graph: &mut InferenceContext, ty: &Type, variables: &mut BTreeMap<BuiltinTypeParameter, TypeId>, requirements: &mut Vec<RequirementId>, reason: crate::sema::inference::ReasonId, origin: Span, depth: usize) -> Result<TypeId, InferenceError> {
    if depth >= 256 { return Err(InferenceError::Limit("registry template depth")); }
    Ok(match ty {
        Type::BuiltinParameter(parameter) => {
            if let Some(id) = variables.get(parameter) { *id }
            else if *parameter == BuiltinTypeParameter::Receiver { return Err(InferenceError::Boundary("Self has no canonical receiver")); }
            else { let id = graph.fresh(1, origin)?; variables.insert(*parameter, id); id }
        }
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => {
            let inner = import_type(graph, inner, variables, requirements, reason, origin, depth + 1)?;
            match ty { Type::List(_) => graph.list(inner)?, Type::Stream(_) => graph.stream(inner)?, _ => graph.optional(inner)? }
        }
        Type::Map(key, value) | Type::Result(key, value) => {
            let key = import_type(graph, key, variables, requirements, reason, origin, depth + 1)?;
            let value = import_type(graph, value, variables, requirements, reason, origin, depth + 1)?;
            if matches!(ty, Type::Map(_, _)) { requirements.push(graph.require_eligibility(Eligibility::MapKey, key, reason)?); graph.map(key, value)? }
            else { graph.result(key, value)? }
        }
        Type::Record(fields) => {
            let fields = fields.iter().map(|(label, ty)| Ok(RowField { label: *label, ty: import_type(graph, ty, variables, requirements, reason, origin, depth + 1)? })).collect::<Result<Vec<_>, InferenceError>>()?;
            let row = graph.row(fields, None)?;
            graph.record(row)?
        }
        Type::Module(fields) => {
            let fields = fields.iter().map(|(label, export)| {
                let ModuleExportType::Value { ty, optional } = export else { return Err(InferenceError::Boundary("registry module template contains an undeclared callable relationship")); };
                Ok(ModuleField { label: *label, ty: import_type(graph, ty, variables, requirements, reason, origin, depth + 1)?, optional: *optional })
            }).collect::<Result<Vec<_>, InferenceError>>()?;
            graph.module(fields)?
        }
        Type::Unknown | Type::Invalid | Type::Inference(_) | Type::Graph(_) => return Err(InferenceError::Boundary("canonical registry template contains an unresolved type")),
        scalar => graph.import_type(scalar, 1, origin)?,
    })
}

fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""), '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"), '\r' => out.push_str("\\r"), '\t' => out.push_str("\\t"),
            character if character < ' ' => out.push_str(&format!("\\u{:04x}", character as u32)),
            character => out.push(character),
        }
    }
    out.push('"');
    out
}

fn object(mut fields: Vec<(&str, String)>) -> String {
    fields.sort_by_key(|(key, _)| *key);
    format!("{{{}}}", fields.into_iter().map(|(key, value)| format!("{}:{value}", quote(key))).collect::<Vec<_>>().join(","))
}

fn canonical_type(ty: &Type) -> Result<String, InferenceError> {
    Ok(match ty {
        Type::BuiltinParameter(parameter) => object(vec![("variable", quote(parameter.label()))]),
        Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) => object(vec![("head", quote(match ty { Type::List(_) => "List", Type::Stream(_) => "Stream", _ => "Optional" })), ("arguments", format!("[{}]", canonical_type(inner)?))]),
        Type::Map(key, value) | Type::Result(key, value) => object(vec![("head", quote(if matches!(ty, Type::Map(_, _)) { "Map" } else { "Result" })), ("arguments", format!("[{},{}]", canonical_type(key)?, canonical_type(value)?))]),
        Type::ErasedRecord => object(vec![("head", quote("Record")), ("fields", "{}".into())]),
        Type::DynamicModule => object(vec![("head", quote("Module")), ("fields", "{}".into())]),
        Type::Record(fields) => {
            let fields = fields.iter().map(|(name, ty)| Ok((name.as_str().to_string(), canonical_type(ty)?))).collect::<Result<BTreeMap<_, _>, InferenceError>>()?;
            object(vec![("head", quote("Record")), ("fields", format!("{{{}}}", fields.iter().map(|(name, value)| format!("{}:{value}", quote(name))).collect::<Vec<_>>().join(",")))])
        }
        Type::Module(fields) => {
            let fields = fields.iter().map(|(name, export)| {
                let ModuleExportType::Value { ty, optional:false } = export else { return Err(InferenceError::Boundary("canonical registry module contains unsupported export metadata")); };
                Ok((name.as_str().to_string(), canonical_type(ty)?))
            }).collect::<Result<BTreeMap<_, _>, InferenceError>>()?;
            object(vec![("head", quote("Module")), ("fields", format!("{{{}}}", fields.iter().map(|(name, value)| format!("{}:{value}", quote(name))).collect::<Vec<_>>().join(",")))])
        }
        Type::ErrorFamily(name) => object(vec![("head", quote("ErrorFamily")), ("identity", quote(&name.as_str()))]),
        Type::Unknown | Type::Invalid | Type::Inference(_) | Type::Graph(_) => return Err(InferenceError::Boundary("canonical registry identity contains an unresolved type")),
        scalar => object(vec![("head", quote(&format!("{scalar:?}")))]),
    })
}

fn canonical_signature(owner: RegistryOwner, entry: &str, signature: &ModuleFnSig, receiver: Option<&Type>) -> Result<String, InferenceError> {
    let owner = match owner { RegistryOwner::Module(name) => format!("module.{name}"), RegistryOwner::Method(receiver) => format!("method.{}", xsh_registry::signature::receiver_name(receiver)), RegistryOwner::TypeConstructor(name) => format!("constructor.{name}") };
    let parameters = signature.params.iter().map(|parameter| Ok(object(vec![("label", quote(parameter.name)), ("type", canonical_type(&parameter.ty)?), ("defaulted", parameter.defaulted.to_string()), ("mode", quote("positional_or_named")), ("rest", "false".into())]))).collect::<Result<Vec<_>, InferenceError>>()?;
    Ok(object(vec![("owner", quote(&owner)), ("entry", quote(entry)), ("receiver", receiver.map(canonical_type).transpose()?.unwrap_or_else(|| "null".into())),
        ("parameters", format!("[{}]", parameters.join(","))), ("result", canonical_type(&signature.return_ty)?), ("pure", signature.pure.to_string()), ("command", signature.command.to_string()),
        ("argument_check", quote(&format!("{:?}", signature.arg_check))), ("semantic_rule", quote(&format!("{:?}", signature.semantic_rule))),
        ("operation", quote(&format!("{:?}", signature.op))), ("binding", quote(&format!("{:?}", signature.binding))), ("effect", signature.effect.as_ref().map(|effect| quote(effect.as_str())).unwrap_or_else(|| "null".into()))]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::inference::{Atom, RequirementTemplate, ScopedRoot, TypeNode};
    use crate::source::SourceId;
    use std::collections::BTreeSet;

    #[derive(Debug, PartialEq)]
    enum Json { Null, Bool(bool), Number(String), Str(String), Array(Vec<Json>), Object(BTreeMap<String, Json>) }
    impl Json {
        fn from_raw(value: miniserde::json::Value) -> Self {
            match value {
                miniserde::json::Value::Null => Self::Null,
                miniserde::json::Value::Bool(value) => Self::Bool(value),
                value @ miniserde::json::Value::Number(_) => Self::Number(miniserde::json::to_string(&value)),
                miniserde::json::Value::String(value) => Self::Str(value),
                miniserde::json::Value::Array(values) => Self::Array(values.into_iter().map(Self::from_raw).collect()),
                miniserde::json::Value::Object(values) => Self::Object(values.into_iter().map(|(key, value)| (key, Self::from_raw(value))).collect()),
            }
        }
        fn as_str(&self) -> Option<&str> { match self { Self::Str(value) => Some(value), _ => None } }
        fn as_array(&self) -> Option<&[Self]> { match self { Self::Array(values) => Some(values), _ => None } }
        fn is_null(&self) -> bool { matches!(self, Self::Null) }
    }
    impl std::ops::Index<&str> for Json {
        type Output = Json;
        fn index(&self, key: &str) -> &Self::Output { match self { Self::Object(values) => values.get(key).unwrap_or(&Json::Null), _ => &Json::Null } }
    }
    impl PartialEq<String> for Json { fn eq(&self, other: &String) -> bool { self.as_str() == Some(other.as_str()) } }
    impl PartialEq<&str> for Json { fn eq(&self, other: &&str) -> bool { self.as_str() == Some(*other) } }
    fn parse_json(text: &str) -> Json { Json::from_raw(crate::modules::json::parse_raw_json(text).unwrap()) }
    fn span() -> Span { Span::new(SourceId::new(0), 0, 1) }

    #[test]
    fn retained_catalog_exports_refuse_foreign_owners_without_candidate_intersections() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let mut registry = RegistryGraph::default();
        registry.module_family(&mut graph, "time", "now", span()).unwrap();
        registry.schema(&mut graph, "FsEntry", span()).unwrap();
        registry.builtin_error_variant(&mut graph, "AssertionError", "Failed", span()).unwrap();
        let mut language = crate::sema::operation_graph::OperationGraph::default();
        language.binary_family(&mut graph, crate::syntax::node::BinaryOp::Mul, span()).unwrap();
        let mut stages = crate::sema::stage_graph::StageGraph::default();
        stages.family(&mut graph, crate::syntax::node::StreamStageKind::Count, crate::sema::stage_graph::StageForm::default(), span()).unwrap();
        let foreign = InferenceContext::default();
        assert!(matches!(registry.retained_candidates(&foreign, &[]), Err(InferenceError::ForeignHandle)));
        assert!(matches!(registry.retained_schemas(&foreign), Err(InferenceError::ForeignHandle)));
        assert!(matches!(registry.retained_errors(&foreign), Err(InferenceError::ForeignHandle)));
        assert!(matches!(language.retained_candidates(&foreign, &[]), Err(InferenceError::ForeignHandle)));
        assert!(matches!(stages.retained_candidates(&foreign, &[]), Err(InferenceError::ForeignHandle)));
        assert!(RegistryGraph::default().retained_candidates(&foreign, &[]).unwrap().is_empty());
    }
    fn signature(graph: &InferenceContext, ty: TypeId) -> Arrow {
        let TypeNode::Arrow(arrow) = graph.node(graph.resolved(ty).unwrap()).unwrap() else { panic!("not a callable") };
        arrow.clone()
    }

    fn import_catalog(graph: &mut InferenceContext, catalog: &mut RegistryGraph) {
        for (module, signatures) in api_spec().module_entries() {
            for entry in &signatures.functions {
                catalog.module_family(graph, module, entry.name, span()).unwrap_or_else(|error| panic!("{module}.{}: {error:?}", entry.name));
            }
        }
        for (receiver, methods) in api_spec().method_entries() {
            for entry in methods {
                if receiver == MethodReceiver::PathConstructor {
                    catalog.type_constructor_family(graph, "Path", entry.name, span()).unwrap();
                } else {
                    catalog.method_family(graph, entry.name, span()).unwrap_or_else(|error| panic!("method {}: {error:?}", entry.name));
                }
            }
        }
    }

    #[test]
    fn registry_reference_protocol_audit_reconciles_every_module_candidate() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default();
        let inventory = parse_json(include_str!("../../bench/typing/operations.json"));
        let entries = inventory["operations"].as_array().unwrap();
        let mut rows = Vec::new(); let mut matched = BTreeSet::new();
        for (module, signatures) in api_spec().module_entries() {
            for entry in &signatures.functions {
                let family = catalog.module_family(&mut graph, module, entry.name, span()).unwrap();
                let members = graph.family(family).unwrap().to_vec();
                let family_disposition = if members.len() > 1 {
                    let eligible = members.iter().all(|candidate| {
                        let template = graph.candidate(*candidate).unwrap();
                        let scheme = graph.scheme(template.scheme).unwrap();
                        catalog.metadata(&graph, *candidate).unwrap().reference_family_member_supported(template)
                            && scheme.quantifiers.is_empty() && scheme.effect_quantifiers.is_empty()
                    });
                    if eligible {
                        let reason = graph.reason(span(), None).unwrap();
                        match graph.trial(|graph| {
                            let mut instances = Vec::with_capacity(members.len());
                            for &candidate in &members { instances.push((candidate, graph.instantiate(graph.candidate(candidate)?.scheme, 1, reason)?)); }
                            let callable = graph.native_family_callable(family, instances, reason)?;
                            let signature = graph.callable_signature(callable)?;
                            Ok(matches!(graph.node(signature)?, TypeNode::CallableChoice(_)))
                        }) {
                            Ok(false) => "native_finite_family_protocol",
                            Ok(true) => "native_member_choice_protocol",
                            Err(InferenceError::Boundary(_)) => "heterogeneous_family_reference_refusal",
                            Err(error) => panic!("{}: {error:?}", entry.name),
                        }
                    } else { "family_preparation_protocol_refusal" }
                } else { "" };
                for &candidate in &members {
                    let metadata = catalog.metadata(&graph, candidate).unwrap();
                    let template = graph.candidate(candidate).unwrap();
                    assert!(!template.has_receiver);
                    let canonical = parse_json(metadata.identity.as_str().strip_prefix("registry:").unwrap());
                    let family_id = format!("registry.module.{module}.{}", entry.name);
                    let original: Vec<_> = entries.iter().filter(|entry| entry["overload_family_id"] == family_id
                        && entry["runtime_operation"] == canonical["operation"] && entry["parameters"] == canonical["parameters"]
                        && entry["result_behavior"]["declared"] == canonical["result"]).collect();
                    assert_eq!(original.len(), 1, "{}", metadata.public_label.as_str());
                    let id = original[0]["id"].as_str().unwrap(); assert!(matched.insert(id.to_owned()));
                    let disposition = if members.len() != 1 { family_disposition } else { match metadata.reference_protocol(template) {
                        RegistryReferenceProtocol::Arrow => "arrow_reference",
                        RegistryReferenceProtocol::Native => "native_monomorphic_protocol",
                        RegistryReferenceProtocol::Boundary(RegistryReferenceBoundary::ArgumentCheck) => "semantic_binding_reference_refusal",
                        RegistryReferenceProtocol::Boundary(RegistryReferenceBoundary::SemanticRule) => "descriptor_reference_refusal",
                        RegistryReferenceProtocol::Boundary(_) => "source_receiver_protocol_refusal",
                    } };
                    let scheme = graph.scheme(metadata.scheme).unwrap();
                    rows.push(object(vec![("id", quote(id)), ("label", quote(&metadata.public_label.as_str())),
                        ("operation", quote(&format!("{:?}", metadata.operation))), ("members", members.len().to_string()),
                        ("disposition", quote(disposition)), ("argument_check", quote(&format!("{:?}", metadata.argument_check))),
                        ("canonical_contract", if members.len() > 1 { metadata.identity.as_str().strip_prefix("registry:").unwrap().to_owned() } else { "null".to_owned() }),
                        ("semantic_rule", quote(&format!("{:?}", metadata.semantic_rule))),
                        ("eligibility", quote(&format!("{:?}", template.actual_eligibility))),
                        ("input_roles", quote(&format!("{:?}", template.effect_roles))),
                        ("output_roles", quote(&format!("{:?}", template.output_effect_roles))),
                        ("lifecycle", quote(&format!("{:?}", metadata.lifecycle))),
                        ("producer_transfer", quote(&format!("{:?}", metadata.producer_transfer))),
                        ("type_quantifiers", scheme.quantifiers.len().to_string()),
                        ("effect_quantifiers", scheme.effect_quantifiers.len().to_string()),
                        ("scheme_requirements", scheme.requirements.len().to_string())]));
                }
            }
        }
        assert_eq!(matched.len(), if cfg!(feature = "native-tests") { 323 } else { 309 });
        println!("NATIVE_REFERENCE_MATRIX {}", object(vec![("candidate_count", matched.len().to_string()), ("rows", format!("[{}]", rows.join(",")))]));
    }

    #[test]
    fn registry_catalog_reconciles_every_frozen_callable() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default();
        import_catalog(&mut graph, &mut catalog);
        let inventory = parse_json(include_str!("../../bench/typing/operations.json"));
        let entries = inventory["operations"].as_array().unwrap();
        assert_eq!(entries.len(), 730);
        let expected = if cfg!(feature = "native-tests") { 480 } else { 466 };
        assert_eq!(catalog.candidates.len(), expected);
        let mut matched = BTreeSet::new();
        for (id, metadata) in &catalog.candidates {
            let metadata = catalog.metadata(&graph, *id).unwrap_or_else(|error| panic!("{:?}: {error:?}", metadata.identity));
            let json = parse_json(metadata.identity.as_str().strip_prefix("registry:").unwrap());
            let owner = json["owner"].as_str().unwrap().replace("constructor.", "method.");
            let family = format!("registry.{owner}.{}", metadata.entry);
            let matches: Vec<_> = entries.iter().filter(|entry| {
                entry["overload_family_id"] == family && entry["runtime_operation"] == json["operation"] && entry["parameters"] == json["parameters"] && entry["result_behavior"]["declared"] == json["result"]
            }).collect();
            assert_eq!(matches.len(), 1, "{}", metadata.identity.as_str());
            let entry = matches[0];
            assert!(matched.insert(entry["id"].as_str().unwrap()));
            assert_eq!(entry["implementation_binding"], json["binding"]);
            if metadata.operation == RuntimeOp::ProcessCommandArgv {
                assert_eq!(entry["argument_check"].as_str(), Some("Standard"));
                assert_eq!(json["argument_check"].as_str(), Some("CommandArgv"));
            } else {
                assert_eq!(entry["argument_check"], json["argument_check"]);
            }
            if metadata.owner == RegistryOwner::Module("error") && metadata.entry == "fail" {
                assert_eq!(entry["command_eligible"], parse_json("true"));
                assert_eq!(json["command"], parse_json("false"));
            } else {
                assert_eq!(entry["command_eligible"], json["command"]);
            }
            if metadata.owner == RegistryOwner::Module("error") && metadata.entry == "fail" {
                assert_eq!(entry["effect_behavior"]["pure"], parse_json("false"));
                assert_eq!(json["pure"], parse_json("true"));
            } else {
                assert_eq!(entry["effect_behavior"]["pure"], json["pure"]);
            }
            assert_eq!(entry["result_behavior"]["semantic_preparation"], json["semantic_rule"]);
            let effect: Vec<_> = metadata.required_effect.iter().map(|effect| effect.as_str()).collect();
            if matches!(metadata.operation,RuntimeOp::FsStreamLines|RuntimeOp::FsBytesLines) {
                assert_eq!(entry["effect_behavior"]["required"],parse_json("[]"));
                assert_eq!(effect,vec!["fs"]);
            } else if let RegistryOwner::Module(module) = metadata.owner {
                let baseline: Vec<_> = Effect::from_module_call(module, metadata.entry).iter().map(|effect| effect.as_str()).collect();
                assert_eq!(entry["effect_behavior"]["required"], parse_json(&format!("[{}]", baseline.iter().map(|effect| quote(effect)).collect::<Vec<_>>().join(","))));
                if metadata.kind == CallableKind::Pure { assert!(effect.is_empty()); }
                else if matches!(metadata.operation, RuntimeOp::HashMd5 | RuntimeOp::HashSha1 | RuntimeOp::HashSha256 | RuntimeOp::HashSha512) {
                    assert!(baseline.is_empty());
                    assert_eq!(effect, vec!["fs"]);
                } else { assert_eq!(effect, baseline); }
            } else {
                assert_eq!(entry["effect_behavior"]["required"], parse_json(&format!("[{}]", effect.iter().map(|effect| quote(effect)).collect::<Vec<_>>().join(","))));
            }
            if !entry["receiver_template"].is_null() { assert_eq!(entry["receiver_template"], json["receiver"]); }
            let scheme = graph.scheme(metadata.scheme).unwrap();
            graph.validate_scoped(ScopedRoot { ty: scheme.body, scope: Some(metadata.scheme) }).unwrap();
            let arrow = signature(&graph, scheme.body);
            assert_eq!(arrow.kind, metadata.kind);
            if matches!(metadata.lifecycle,RegistryLifecycle::CollectReceiver { materialized:false }) { assert!(matches!(arrow.effects,EffectSummary::Rigid { scope,.. } if scope==metadata.scheme)); }
            else { assert_eq!(arrow.effects, EffectSummary::Closed(effect_bits(metadata.required_effect.as_ref()))); }
            let offset = usize::from(matches!(metadata.owner, RegistryOwner::Method(_)));
            assert_eq!(arrow.params.len(), metadata.parameters.len() + offset);
            for (formal, parameter) in arrow.params[offset..].iter().zip(metadata.parameters.iter()) {
                assert_eq!(formal.label, parameter.label); assert_eq!(formal.defaulted, parameter.defaulted); assert!(!formal.rest);
            }
        }
        assert_eq!(matched.len(), expected);
        for family in catalog.families.values() {
            let ordered = graph.family(*family).unwrap().to_vec();
            let identities: Vec<_> = ordered.iter().map(|id| graph.candidate(*id).unwrap().identity.as_str()).collect();
            assert!(identities.windows(2).all(|pair| pair[0] < pair[1]));
            let reversed: Vec<_> = ordered.into_iter().rev().collect();
            assert_eq!(graph.register_family(&reversed).unwrap(), *family);
        }
        let enabled = if cfg!(feature = "native-tests") { "default" } else { "core" };
        let frozen = entries.iter().filter(|entry| entry["id"].as_str().is_some_and(|id| id.starts_with("registry.module.") || id.starts_with("registry.method.")) && entry["enabled_configurations"].as_array().unwrap().iter().any(|configuration| configuration.as_str() == Some(enabled))).count();
        assert_eq!(frozen, expected);
    }

    #[test]
    fn registry_every_candidate_proves_its_contract_and_rejects_extra_arguments() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default();
        import_catalog(&mut graph, &mut catalog);
        let mut ids: Vec<_> = catalog.candidates.keys().copied().collect();
        ids.sort_by_key(|id| catalog.metadata(&graph, *id).unwrap().identity.as_str());
        let why = graph.reason(span(), None).unwrap(); let string = graph.atom(Atom::Str).unwrap(); let unit = graph.atom(Atom::Unit).unwrap();
        for id in ids {
            let template = graph.candidate(id).unwrap().clone(); let identity = template.identity.as_str();
            let instance = graph.instantiate(template.scheme, 1, why).unwrap();
            for substitution in &instance.substitutions { graph.unify(*substitution, string, why).unwrap(); }
            graph.solve().unwrap();
            let arrow = signature(&graph, instance.ty); let offset = usize::from(template.has_receiver);
            let input_roles=template.effect_roles.iter().map(|(role,reference)|(*role,EffectSummary::Closed(match reference { EffectRoleReference::Binder(index)=>graph.scheme(template.scheme).unwrap().effect_quantifiers[*index as usize].lower,EffectRoleReference::Fixed(bits)=>*bits }))).collect();
            let family=graph.register_family(&[id]).unwrap(); let outputs=catalog.output_effect_bindings(&mut graph,family,1).unwrap();
            let call = crate::sema::inference::OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:outputs, effect_bindings: input_roles,
                receiver: if template.has_receiver { Some(arrow.params[0].ty) } else { None },
                arguments: arrow.params[offset..].iter().map(|parameter| Some(parameter.ty)).collect(), result: arrow.result, effects: arrow.effects,
            };
            let family = graph.register_family(&[id]).unwrap();
            let requirement = graph.require_operation(family, call.clone(), why).unwrap();
            graph.solve().unwrap_or_else(|error| panic!("{identity}: {error:?}"));
            assert_eq!(graph.candidate_evidence(requirement).unwrap().unwrap().candidate, id, "{identity}");
            let mut invalid = call.clone(); invalid.arguments.push(Some(unit));
            assert!(graph.trial(|graph| { graph.require_operation(family, invalid, why)?; graph.solve() }).is_err(), "extra argument accepted: {identity}");
            if let Some(required) = arrow.params[offset..].iter().position(|parameter| !parameter.defaulted) {
                let mut invalid = call.clone(); invalid.arguments[required] = None;
                assert!(graph.trial(|graph| { graph.require_operation(family, invalid, why)?; graph.solve() }).is_err(), "missing required argument accepted: {identity}");
            }
            // Checksum alternatives share a displayed defaulted slot, while
            // source binding requires exactly one named checksum.
            if catalog.metadata(&graph, id).unwrap().argument_check != ApiArgCheck::HashVerifyFile && arrow.params[offset..].iter().any(|parameter| parameter.defaulted) {
                let mut defaulted = call;
                for (argument, parameter) in defaulted.arguments.iter_mut().zip(&arrow.params[offset..]) { if parameter.defaulted { *argument = None; } }
                let requirement = graph.require_operation(family, defaulted, why).unwrap(); graph.solve().unwrap_or_else(|error| panic!("defaults {identity}: {error:?}"));
                assert_eq!(graph.candidate_evidence(requirement).unwrap().unwrap().candidate, id);
            }
        }
    }

    #[test]
    fn registry_receiver_self_and_occurrence_substitutions_remain_connected() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default();
        let family = catalog.method_family(&mut graph, "context", span()).unwrap();
        let id = graph.family(family).unwrap()[0];
        assert_eq!(catalog.metadata(&graph, id).unwrap().operation, RuntimeOp::ResultContext);
        let scheme = graph.candidate(id).unwrap().scheme; let why = graph.reason(span(), None).unwrap();
        let first = graph.instantiate(scheme, 1, why).unwrap(); let second = graph.instantiate(scheme, 1, why).unwrap();
        let first = signature(&graph, first.ty); let second = signature(&graph, second.ty);
        assert_eq!(first.params[0].ty, first.result); assert_eq!(second.params[0].ty, second.result);
        assert_ne!(first.result, second.result);
        for (arrow, atom) in [(first, Atom::Int), (second, Atom::Bool)] {
            let value = graph.atom(atom).unwrap(); let error = graph.atom(Atom::Error).unwrap(); let actual = graph.result(value, error).unwrap();
            graph.unify(arrow.params[0].ty, actual, why).unwrap();
            assert_eq!(graph.export_type(arrow.result).unwrap(), Type::Result(Box::new(match atom { Atom::Int => Type::Int, _ => Type::Bool }), Box::new(Type::Error)));
        }
        let family = catalog.module_family(&mut graph, "map", "empty", span()).unwrap(); let id = graph.family(family).unwrap()[0]; let scheme = graph.candidate(id).unwrap().scheme;
        assert_eq!(graph.scheme(scheme).unwrap().quantifiers.len(), 2);
        let first = graph.instantiate(scheme, 1, why).unwrap(); let second = graph.instantiate(scheme, 1, why).unwrap();
        let a = signature(&graph, first.ty); let b = signature(&graph, second.ty);
        let TypeNode::Map(first_key, first_value) = *graph.node(a.result).unwrap() else { panic!() };
        let TypeNode::Map(second_key, second_value) = *graph.node(b.result).unwrap() else { panic!() };
        assert_ne!(first_key, second_key); assert_ne!(first_value, second_value);
        assert!(graph.export_type(a.result).is_err());
        assert!(first.requirements.iter().any(|id| matches!(graph.requirement_template(*id).unwrap(), RequirementTemplate::Eligibility { predicate: Eligibility::MapKey, ty } if ty == first_key)));
        let invalid = graph.atom(Atom::Float).unwrap();
        assert!(graph.probe(|graph| { graph.unify(first_key, invalid, why)?; graph.solve() }).is_err());
        let string = graph.atom(Atom::Str).unwrap(); let path = graph.atom(Atom::Path).unwrap(); graph.unify(second_key, string, why).unwrap(); graph.unify(second_value, path, why).unwrap(); graph.solve().unwrap();
        assert_eq!(graph.export_type(b.result).unwrap(), Type::Map(Box::new(Type::Str), Box::new(Type::Path)));
    }

    #[test]
    fn registry_map_templates_keep_every_sealed_key_domain() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default();
        let family = catalog.module_family(&mut graph, "map", "empty", span()).unwrap(); let candidate = graph.family(family).unwrap()[0]; let scheme = graph.candidate(candidate).unwrap().scheme;
        let why = graph.reason(span(), None).unwrap(); let value = graph.atom(Atom::Bool).unwrap();
        for atom in [Atom::Str, Atom::Int, Atom::UInt, Atom::Bool, Atom::Bytes, Atom::Path, Atom::Duration] {
            let instance = graph.instantiate(scheme, 1, why).unwrap(); let arrow = signature(&graph, instance.ty);
            let TypeNode::Map(key, item) = *graph.node(arrow.result).unwrap() else { panic!() };
            let actual = graph.atom(atom).unwrap(); graph.unify(key, actual, why).unwrap(); graph.unify(item, value, why).unwrap(); graph.solve().unwrap();
            assert!(instance.requirements.iter().all(|requirement| graph.eligibility_satisfied(*requirement).unwrap()));
            assert_eq!(graph.resolved(key).unwrap(), actual);
        }
        for atom in [Atom::Null, Atom::Float, Atom::Any, Atom::Digest, Atom::Status, Atom::Unit, Atom::Error] {
            let actual = graph.atom(atom).unwrap();
            assert!(graph.trial(|graph| {
                let instance = graph.instantiate(scheme, 1, why)?; let arrow = signature(graph, instance.ty);
                let TypeNode::Map(key, _) = *graph.node(arrow.result)? else { return Err(InferenceError::KindMismatch) };
                graph.unify(key, actual, why)?; graph.solve()
            }).is_err(), "{atom:?}");
        }
    }

    #[test]
    fn registry_actual_json_predicates_preserve_supplied_operand_types() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default();
        for (entry, slots) in [("encode", vec![0]), ("encode_lines", vec![0]), ("write", vec![1]), ("write_lines", vec![1]), ("set", vec![0, 2])] {
            let family = catalog.module_family(&mut graph, "json", entry, span()).unwrap();
            for id in graph.family(family).unwrap() { assert_eq!(graph.candidate(*id).unwrap().actual_eligibility, slots.iter().map(|slot| (*slot, Eligibility::JsonCompatible)).collect::<Vec<_>>()); }
        }
        let why = graph.reason(span(), None).unwrap();
        for ty in [Type::Path, Type::Bytes, Type::List(Box::new(Type::Path)), Type::Map(Box::new(Type::Int), Box::new(Type::Str))] {
            let actual = graph.import_type(&ty, 1, span()).unwrap();
            assert!(graph.probe(|graph| { graph.require_eligibility(Eligibility::JsonCompatible, actual, why)?; graph.solve() }).is_err(), "{ty:?}");
        }
        for ty in [Type::Any, Type::List(Box::new(Type::Str)), Type::Map(Box::new(Type::Str), Box::new(Type::Bool))] {
            let actual = graph.import_type(&ty, 1, span()).unwrap(); let requirement = graph.require_eligibility(Eligibility::JsonCompatible, actual, why).unwrap(); graph.solve().unwrap(); assert!(graph.eligibility_satisfied(requirement).unwrap());
        }
    }

    #[test]
    fn registry_candidate_selection_uses_actual_json_types_and_exact_receivers() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default();
        let family = catalog.module_family(&mut graph, "json", "encode_lines", span()).unwrap(); let why = graph.reason(span(), None).unwrap();
        let string = graph.atom(Atom::Str).unwrap(); let path = graph.atom(Atom::Path).unwrap(); let bad = graph.list(path).unwrap(); let good = graph.list(string).unwrap();
        let error = graph.atom(Atom::Error).unwrap(); let result = graph.result(string, error).unwrap();
        let call = |argument| crate::sema::inference::OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), effect_bindings: Vec::new(), receiver: None, arguments: vec![Some(argument)], result, effects: EffectSummary::Closed(EffectSet::EMPTY) };
        assert!(graph.trial(|graph| { graph.require_operation(family, call(bad), why)?; graph.solve() }).is_err());
        let requirement = graph.require_operation(family, call(good), why).unwrap(); graph.solve().unwrap();
        assert_eq!(catalog.metadata(&graph, graph.candidate_evidence(requirement).unwrap().unwrap().candidate).unwrap().operation, RuntimeOp::JsonEncodeLines);
        let family = catalog.method_family(&mut graph, "len", span()).unwrap();
        let forward = graph.family(family).unwrap().to_vec(); assert!(forward.len() > 1);
        let reverse: Vec<_> = forward.into_iter().rev().collect(); let reverse = graph.register_family(&reverse).unwrap(); assert_eq!(reverse, family);
        let bytes = graph.atom(Atom::Bytes).unwrap(); let int = graph.atom(Atom::Int).unwrap();
        for family in [family, reverse] {
            let requirement = graph.require_operation(family, crate::sema::inference::OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), effect_bindings: Vec::new(), receiver: Some(bytes), arguments: vec![], result: int, effects: EffectSummary::Closed(EffectSet::EMPTY) }, why).unwrap();
            graph.solve().unwrap();
            let metadata = catalog.metadata(&graph, graph.candidate_evidence(requirement).unwrap().unwrap().candidate).unwrap();
            assert_eq!(metadata.owner, RegistryOwner::Method(MethodReceiver::Bytes));
            assert_eq!(metadata.operation, RuntimeOp::BytesLen);
        }
    }

    #[test]
    fn registry_producer_transfers_follow_native_value_paths_and_reject_foreign_candidates() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default();
        import_catalog(&mut graph, &mut catalog);
        let candidate = |operation, owner| catalog.candidates.iter().find_map(|(&candidate, metadata)| (metadata.operation == operation && metadata.owner == owner).then_some(candidate)).unwrap();
        let push = candidate(RuntimeOp::ListPush, RegistryOwner::Method(MethodReceiver::List));
        let plan = catalog.producer_result_transfer(&graph, push).unwrap();
        assert_eq!(plan, &RegistryProducerTransferPlan::Transfers(vec![
            RegistryProducerTransfer { input: RegistryProducerInput::Receiver, input_path: vec![].into(), output_path: vec![].into() },
            RegistryProducerTransfer { input: RegistryProducerInput::Argument(0), input_path: vec![].into(), output_path: vec![RegistryProducerComponent::ListItem].into() },
        ].into()));
        let get = candidate(RuntimeOp::MapGet, RegistryOwner::Method(MethodReceiver::Map));
        assert_eq!(catalog.producer_result_transfer(&graph, get).unwrap(), &RegistryProducerTransferPlan::Transfers(vec![
            RegistryProducerTransfer { input: RegistryProducerInput::Receiver, input_path: vec![RegistryProducerComponent::MapValue].into(), output_path: vec![RegistryProducerComponent::ResultSuccess].into() },
        ].into()));
        for owner in [MethodReceiver::Str, MethodReceiver::Bytes] {
            let lines = catalog.candidates.iter().find_map(|(&candidate, metadata)| (metadata.entry == "lines" && metadata.owner == RegistryOwner::Method(owner)).then_some(candidate)).unwrap();
            assert_eq!(catalog.producer_result_transfer(&graph, lines).unwrap(), &RegistryProducerTransferPlan::Empty);
        }
        let record_get = candidate(RuntimeOp::RecordGet, RegistryOwner::Method(MethodReceiver::Record));
        assert_eq!(catalog.producer_result_transfer(&graph, record_get).unwrap(), &RegistryProducerTransferPlan::Opaque);
        assert!(matches!(catalog.producer_result_transfer(&InferenceContext::default(), push), Err(InferenceError::ForeignHandle)));
    }

    #[test]
    fn registry_cache_rejects_foreign_and_retired_graph_handles() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default(); let mut retired = None;
        let result: Result<(), _> = graph.probe(|graph| { retired = Some(catalog.module_family(graph, "map", "empty", span())?); Err(InferenceError::InvalidScheme) });
        assert!(result.is_err()); let live = catalog.module_family(&mut graph, "map", "empty", span()).unwrap();
        assert_ne!(live, retired.unwrap()); assert!(graph.family(retired.unwrap()).is_err());
        assert_eq!(catalog.module_family(&mut graph, "map", "empty", span()).unwrap(), live);
        let mut foreign = InferenceContext::default(); assert_eq!(catalog.module_family(&mut foreign, "map", "empty", span()), Err(InferenceError::ForeignHandle));
        let family = catalog.type_constructor_family(&mut graph, "Path", "parse_bytes", span()).unwrap();
        for id in graph.family(family).unwrap() { let template = graph.candidate(*id).unwrap(); assert!(!template.has_receiver); assert_eq!(signature(&graph, graph.scheme(template.scheme).unwrap().body).params.len(), 1); }
        assert!(catalog.method_family(&mut graph, "parse_bytes", span()).is_err());
    }

    #[test]
    fn registry_failed_family_import_retires_all_partial_candidates() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = RegistryGraph::default(); catalog.bind_owner(&graph).unwrap();
        let (_, module) = api_spec().module_entries().find(|(name, _)| *name == "map").unwrap();
        let entry = module.functions.iter().find(|entry| entry.name == "empty").unwrap(); let signature = &entry.overloads[0];
        let mut partial = None;
        let result = catalog.build_family(&mut graph, "module.map.empty".into(), |catalog, graph| {
            partial = Some(catalog.import_candidate(graph, RegistryOwner::Module("map"), entry.name, signature, None, span())?);
            Err(InferenceError::InvalidScheme)
        });
        assert_eq!(result, Err(InferenceError::InvalidScheme));
        assert!(graph.candidate(partial.unwrap()).is_err()); assert!(catalog.candidates.is_empty()); assert!(catalog.families.is_empty());
        let family = catalog.module_family(&mut graph, "map", "empty", span()).unwrap();
        assert_ne!(graph.family(family).unwrap()[0], partial.unwrap());
        assert_eq!(catalog.candidates.len(), 1);
    }

    #[test]
    fn registry_identity_serializer_preserves_control_and_unicode_labels() {
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let label = "\"\\\n\r\t\u{0001}λ";
        assert_eq!(parse_json(&quote(label)).as_str().unwrap(), label);
        let fields = BTreeMap::from([(Name::intern(label), Type::Int)]);
        let json = parse_json(&canonical_type(&Type::Record(fields)).unwrap());
        assert_eq!(json["fields"][label]["head"], "Int");
    }
    #[test]
    fn path_line_producer_creation_requires_filesystem_permission() {
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter();
        let mut graph=InferenceContext::default(); let mut catalog=RegistryGraph::default();
        for (entry,operation) in [("lines",RuntimeOp::FsStreamLines),("bytes_lines",RuntimeOp::FsBytesLines)] {
            let family=catalog.method_family(&mut graph,entry,span()).unwrap();
            let candidate=graph.family(family).unwrap().iter().copied().find(|candidate|catalog.metadata(&graph,*candidate).unwrap().operation==operation).unwrap();
            let template=graph.candidate(candidate).unwrap(); let arrow=signature(&graph,graph.scheme(template.scheme).unwrap().body);
            assert_eq!(arrow.effects,EffectSummary::Closed(EffectSet::FS),"{operation:?} opens a file during creation");
        }
    }

    #[test]
    fn registry_producer_lifecycles_reconcile_all_frozen_stream_results() {
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter();
        let mut graph=InferenceContext::default(); let mut catalog=RegistryGraph::default(); import_catalog(&mut graph,&mut catalog);
        let inventory=parse_json(include_str!("../../bench/typing/operations.json"));
        let expected:BTreeSet<_>=inventory["operations"].as_array().unwrap().iter().filter(|entry|entry["id"].as_str().is_some_and(|id|id.starts_with("registry."))&&entry["result_behavior"]["declared"]["head"]=="Result"&&entry["result_behavior"]["declared"]["arguments"].as_array().is_some_and(|args|args[0]["head"]=="Stream")).map(|entry|entry["id"].as_str().unwrap()).collect();
        assert_eq!(expected.len(),27);
        let mut count=[0;3]; let mut consumers=0; let mut observed=BTreeSet::new();
        for metadata in catalog.candidates.values() {
            match metadata.lifecycle {
                RegistryLifecycle::ResultProducer { pull,close }=>{
                    assert_eq!(close,EffectSet::EMPTY); let index=if pull==EffectSet::FS {0} else if pull==EffectSet::PROCESS {1} else {assert_eq!(pull,EffectSet::EMPTY);2}; count[index]+=1;
                    let json=parse_json(metadata.identity.as_str().strip_prefix("registry:").unwrap()); let family=format!("registry.{}.{}",json["owner"].as_str().unwrap(),metadata.entry);
                    let matches:Vec<_>=inventory["operations"].as_array().unwrap().iter().filter(|entry|entry["overload_family_id"]==family&&entry["runtime_operation"]==json["operation"]&&entry["parameters"]==json["parameters"]).collect();
                    assert_eq!(matches.len(),1); assert!(observed.insert(matches[0]["id"].as_str().unwrap()));
                    let arrow=signature(&graph,graph.scheme(metadata.scheme).unwrap().body); let TypeNode::Result(producer,_)=graph.node(arrow.result).unwrap() else { panic!() }; assert!(matches!(graph.node(*producer).unwrap(),TypeNode::Stream(_)));
                }
                RegistryLifecycle::CollectReceiver { materialized }=>{ consumers+=1; assert_eq!(materialized,metadata.owner==RegistryOwner::Method(MethodReceiver::List)); assert_eq!(metadata.operation,RuntimeOp::StreamCollect); }
                RegistryLifecycle::None=>{}
            }
        }
        assert_eq!(observed,expected); assert_eq!(count,[8,10,9]); assert_eq!(consumers,2);
        for family in catalog.families.values() {
            let relation=catalog.producer_result_relation(&graph,*family).unwrap(); let members=graph.family(*family).unwrap(); let has_native=members.iter().any(|candidate|matches!(catalog.metadata(&graph,*candidate).unwrap().lifecycle,RegistryLifecycle::ResultProducer { .. })); assert_eq!(relation.is_some(),has_native);
            if let Some(relation)=relation { assert_eq!(relation.alternatives.len(),members.len()); for alternative in &relation.alternatives { assert_eq!(alternative.native_path.is_some(),matches!(catalog.metadata(&graph,alternative.candidate).unwrap().lifecycle,RegistryLifecycle::ResultProducer { .. })); } }
        }
    }

    #[test]
    fn collect_consumes_pull_and_cleanup_permissions_without_changing_item_type() {
        use crate::sema::inference::OperationCall;
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter(); let mut graph=InferenceContext::default(); let mut catalog=RegistryGraph::default(); let why=graph.reason(span(),None).unwrap();
        let family=catalog.method_family(&mut graph,"collect",span()).unwrap(); let int=graph.atom(Atom::Int).unwrap(); let stream=graph.stream(int).unwrap(); let list=graph.list(int).unwrap();
        for (receiver,pull,close,allowed,valid) in [(stream,EffectSet::NET,EffectSet::PROCESS,EffectSet(EffectSet::NET.0|EffectSet::PROCESS.0),true),(stream,EffectSet::NET,EffectSet::PROCESS,EffectSet::NET,false),(list,EffectSet::EMPTY,EffectSet::EMPTY,EffectSet::EMPTY,true),(list,EffectSet::NET,EffectSet::EMPTY,EffectSet::NET,false)] {
            let outcome=graph.trial(|graph| { let requirement=graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver:Some(receiver),arguments:Vec::new(),result:list,effects:EffectSummary::Closed(allowed),effect_bindings:vec![(EffectRole::Pull { source:0 },EffectSummary::Closed(pull)),(EffectRole::Close { source:0 },EffectSummary::Closed(close))],output_effect_bindings:Vec::new() },why)?; graph.solve()?; let evidence=graph.candidate_evidence(requirement)?.ok_or(InferenceError::Boundary("collect remains ambiguous"))?;
                let expected=EffectSet(pull.0|close.0); assert_eq!(graph.closed_effect_summary(evidence.effects)?,EffectSummary::Closed(expected)); assert_eq!(graph.resolved(evidence.result)?,list); Ok(()) });
            assert_eq!(outcome.is_ok(),valid,"{receiver:?}: {outcome:?}");
        }
    }

    #[test]
    fn generic_lines_receiver_keeps_creation_and_result_producer_domains_independent() {
        use crate::sema::inference::OperationCall;
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter(); let mut graph=InferenceContext::default(); let mut catalog=RegistryGraph::default(); let why=graph.reason(span(),None).unwrap();
        let family=catalog.method_family(&mut graph,"lines",span()).unwrap(); let outputs=catalog.output_effect_bindings(&mut graph,family,1).unwrap(); assert_eq!(outputs.len(),2);
        let relation=catalog.producer_result_relation(&graph,family).unwrap().unwrap(); assert_eq!(&*relation.output_roles,&[ProducerRole::Pull,ProducerRole::Close]);
        assert_eq!(relation.alternatives.len(),graph.family(family).unwrap().len()); assert_eq!(relation.alternatives.iter().filter(|alternative|alternative.native_path.is_some()).count(),1);
        let receiver=graph.fresh(1,span()).unwrap(); let result=graph.fresh(1,span()).unwrap(); let creation=EffectSummary::Variable(graph.fresh_derived_effect_at(1,None).unwrap());
        let requirement=graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver:Some(receiver),arguments:Vec::new(),result,effects:creation,effect_bindings:Vec::new(),output_effect_bindings:outputs.clone() },why).unwrap(); graph.solve().unwrap(); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
        let body=graph.arrow(Arrow { kind:CallableKind::Proc,params:vec![Parameter { label:Name::intern("value"),ty:receiver,defaulted:false,rest:false }],result,effects:creation }).unwrap(); let roots:Vec<_>=outputs.iter().map(|(_,root)|*root).collect();
        let scheme=graph.generalize_with_effect_roots(body,0,Generalization::Allowed,&[requirement],&roots).unwrap(); let text=graph.instantiate(scheme,1,why).unwrap(); let file=graph.instantiate(scheme,1,why).unwrap();
        for (instance,atom,pull,operation) in [(text,Atom::Str,EffectSet::EMPTY,RuntimeOp::TextStreamLines),(file,Atom::Path,EffectSet::FS,RuntimeOp::FsStreamLines)] {
            let arrow=signature(&graph,instance.ty); let actual=graph.atom(atom).unwrap(); graph.unify(arrow.params[0].ty,actual,why).unwrap(); graph.solve().unwrap();
            let evidence=instance.requirements.iter().find_map(|requirement|graph.candidate_evidence(*requirement).unwrap()).unwrap(); assert_eq!(catalog.metadata(&graph,evidence.candidate).unwrap().operation,operation);
            let alternative=relation.alternatives.iter().find(|alternative|alternative.candidate==evidence.candidate).unwrap(); assert_eq!(alternative.native_path,if atom==Atom::Path {Some(RegistryProducerPath::ResultSuccess)} else {None});
            assert_eq!(graph.closed_effect_summary(evidence.effects).unwrap(),EffectSummary::Closed(pull)); assert_eq!(graph.closed_effect_summary(instance.effect_roots[0]).unwrap(),EffectSummary::Closed(pull)); assert_eq!(graph.closed_effect_summary(instance.effect_roots[1]).unwrap(),EffectSummary::Closed(EffectSet::EMPTY));
        }
    }

    #[test]
    fn registry_schema_and_error_metadata_reconcile_all_remaining_authorities() {
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter(); let mut graph=InferenceContext::default(); let mut catalog=RegistryGraph::default(); let inventory=parse_json(include_str!("../../bench/typing/operations.json"));
        let entries=inventory["operations"].as_array().unwrap(); let mut observed=BTreeSet::new();
        for (name,declared) in xsh_registry::records::record_schemas() {
            let schema=catalog.schema(&mut graph,name,span()).unwrap(); let matches:Vec<_>=entries.iter().filter(|entry|entry["id"]==schema.authority_id.as_str().to_string()).collect(); assert_eq!(matches.len(),1); let expected=matches[0];
            assert!(observed.insert(schema.authority_id.as_str())); assert_eq!(schema.label.as_str(),name); assert_eq!(expected["classification"],"dynamic_boundary");
            let ty=crate::modules::signature::convert_type(&declared); assert_eq!(graph.export_type(schema.shape).unwrap(),ty); assert_eq!(expected["schema"],parse_json(&canonical_type(&ty).unwrap()));
            let why=graph.reason(span(),None).unwrap(); let any=graph.atom(crate::sema::inference::Atom::Any).unwrap(); assert!(graph.trial(|graph|graph.assignable(schema.shape,any,why)).is_err(),"{name} must retain explicit validation");
        }
        let schema_count=observed.len(); assert_eq!(schema_count,if cfg!(feature="native-tests") {73} else {71});
        for family in xsh_registry::errors::builtin_error_families() { for variant in family.variants {
            let error=catalog.builtin_error_variant(&mut graph,family.name,variant.name,span()).unwrap(); let matches:Vec<_>=entries.iter().filter(|entry|entry["id"]==error.authority_id.as_str().to_string()).collect(); assert_eq!(matches.len(),1); let expected=matches[0]; assert!(observed.insert(error.authority_id.as_str()));
            assert_eq!(error.family.as_str(),family.name); assert_eq!(error.variant.as_str(),variant.name); assert_eq!(expected["classification"],"monomorphic"); assert_eq!(graph.export_type(error.result).unwrap(),Type::ErrorVariant { family:error.family,variant:error.variant });
            let parameters=expected["parameters"].as_array().unwrap(); assert_eq!(parameters.len(),error.parameters.len());
            for ((actual,field),expected) in error.parameters.iter().zip(&family.fields).zip(parameters) { assert_eq!(actual.label.as_str(),field.name); assert_eq!(expected["label"],field.name); let ty=crate::modules::signature::convert_type(&field.ty); assert_eq!(graph.export_type(actual.ty).unwrap(),ty); assert_eq!(expected["type"],parse_json(&canonical_type(&ty).unwrap())); }
            assert_eq!(expected["facets"],parse_json(&format!("[{}]",error.facets.iter().map(|facet|quote(&facet.as_str())).collect::<Vec<_>>().join(","))));
            let why=graph.reason(span(),None).unwrap(); let any=graph.atom(crate::sema::inference::Atom::Any).unwrap(); assert!(graph.trial(|graph|graph.assignable(error.result,any,why)).is_err());
        } }
        assert_eq!(observed.len()-schema_count,17); assert_eq!(observed.len(),schema_count+17);
        assert!(catalog.schema(&mut graph,"MissingSchema",span()).is_err()); assert!(catalog.builtin_error_variant(&mut graph,"ProcessError","MissingVariant",span()).is_err()); assert!(catalog.builtin_error_variant(&mut graph,"MissingFamily","Failed",span()).is_err());
    }

    #[test]
    fn registry_boundary_metadata_reject_foreign_and_retired_type_handles() {
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter(); let mut graph=InferenceContext::default(); let mut catalog=RegistryGraph::default();
        let (mut schema,mut error)=(None,None);
        graph.trial(|graph| { schema=Some(catalog.schema(graph,"FsEntry",span())?); error=Some(catalog.builtin_error_variant(graph,"AssertionError","Failed",span())?); Ok(()) }).unwrap(); let schema=schema.unwrap(); let error=error.unwrap();
        assert!(graph.node(schema.shape).is_err()); assert!(graph.node(error.result).is_err());
        let replacement=catalog.schema(&mut graph,"FsEntry",span()).unwrap(); assert_ne!(replacement.shape,schema.shape); let replacement=catalog.builtin_error_variant(&mut graph,"AssertionError","Failed",span()).unwrap(); assert_ne!(replacement.result,error.result);
        let mut foreign=InferenceContext::default(); assert!(matches!(catalog.schema(&mut foreign,"FsEntry",span()),Err(InferenceError::ForeignHandle))); assert!(matches!(catalog.builtin_error_variant(&mut foreign,"AssertionError","Failed",span()),Err(InferenceError::ForeignHandle)));
    }

}
