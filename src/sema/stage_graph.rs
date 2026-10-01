use crate::sema::inference::{ArgumentRelation, Arrow, Atom, CallableKind, CandidateId, CandidateTemplate, EffectProjection, EffectRole, EffectRoleReference, EffectSet, EffectSummary, Eligibility, Generalization, GraphOwner, InferenceContext, InferenceError, OperationFamilyId, Parameter, ProducerRole, RequirementId, RequirementTemplate, RowField, SchemeId, TypeId};
use crate::source::Span;
use crate::symbol::Name;
use crate::syntax::node::StreamStageKind;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use xsh_registry::stream_parameters::{StageParameter, StageParameterDefault, StageParameterType, stage_parameters};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SequenceDomain { List, Stream }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StageSource { Sequence { domain: SequenceDomain, outer_result: bool }, Str, Bytes }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallbackShape { Absent, Value, Unit, ResultUnit, List, Stream, ResultList, ResultStream }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReductionModes(u8);
impl ReductionModes {
    pub const SUM:Self=Self(1); pub const MIN:Self=Self(2); pub const MAX:Self=Self(4); pub const ALL:Self=Self(7);
    pub fn from_static_configuration(sum:Option<bool>,min:Option<bool>,max:Option<bool>)->Result<Self,InferenceError> {
        let values=[sum,min,max]; let fixed:Vec<_>=values.iter().enumerate().filter_map(|(index,value)|(*value==Some(true)).then_some(index)).collect();
        if fixed.len()>1 { return Err(InferenceError::Boundary("reduce-by enables more than one reduction mode")); }
        if let Some(index)=fixed.first() { return Ok(Self(1<<index)); }
        let bits=values.iter().enumerate().fold(0,|bits,(index,value)|if value.is_none() { bits|(1<<index) } else { bits });
        if bits==0 { return Err(InferenceError::Boundary("reduce-by enables no reduction mode")); } Ok(Self(bits))
    }
    fn contains_sum(self)->bool { self.0&Self::SUM.0!=0 }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReductionShape { Value, Int, UInt, Float, Record }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StageForm {
    pub callback_kind: Option<CallableKind>,
    // Preserve the actual callable value while the candidate's positional
    // item/result/effect contract proves its exact selected invocation.
    pub callback_protocol: bool,
    pub batch_argv: bool,
    pub reduction_modes: Option<ReductionModes>,
}
impl Default for StageForm {
    fn default() -> Self { Self { callback_kind: None, callback_protocol: false, batch_argv: false, reduction_modes: None } }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StageVariant { pub callback_shape:CallbackShape, pub other_sequence:Option<SequenceDomain>, pub reduction_shape:ReductionShape }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdditionalProducer { ZipArgument, CallbackResult }
#[derive(Clone, Debug)]
pub(crate) struct StageCandidate {
    pub identity: Name,
    pub scheme: SchemeId,
    pub stage: StreamStageKind,
    pub source: StageSource,
    pub input_numeric: Option<Atom>,
    pub form: StageForm,
    pub variant: StageVariant,
    pub parameters: &'static [StageParameter],
    pub callback_slot: Option<usize>,
    pub additional_producer: Option<AdditionalProducer>,
    pub effects: StageEffectRelation,
}

#[derive(Default)]
pub(crate) struct StageGraph {
    owner: Option<GraphOwner>,
    families: BTreeMap<String, OperationFamilyId>,
    candidates: FxHashMap<CandidateId, StageCandidate>,
}

#[derive(Clone, Debug)]
pub(crate) struct StageEffectRelation {
    pub output_pull: Box<[EffectRole]>,
    pub output_close: Box<[EffectRole]>,
    pub intrinsic_pull: EffectSet,
}

impl StageGraph {
    pub(crate) fn family(&mut self, graph: &mut InferenceContext, stage: StreamStageKind, form: StageForm, origin: Span) -> Result<OperationFamilyId, InferenceError> {
        validate_form(&stage,form)?;
        if self.owner.is_some_and(|owner|owner!=graph.owner()) { return Err(InferenceError::ForeignHandle); } self.owner=Some(graph.owner());
        let key=format!("{}:{form:?}",stage.as_str());
        if let Some(family)=self.families.get(&key).copied() { if graph.family(family).is_ok() { return Ok(family); } }
        self.families.retain(|_,id|graph.family(*id).is_ok()); self.candidates.retain(|id,_|graph.candidate(*id).is_ok());
        let result=graph.probe(|graph| {
            let sources=match stage { StreamStageKind::TextStreamLines|StreamStageKind::JsonLines|StreamStageKind::JsonStream=>vec![StageSource::Str],StreamStageKind::BytesChunks=>vec![StageSource::Bytes],_=>[SequenceDomain::List,SequenceDomain::Stream].into_iter().flat_map(|domain|[false,true].map(|outer_result|StageSource::Sequence { domain,outer_result })).collect() };
            let mut candidates=Vec::new();
            for source in sources {
                // Dynamic items keep the reducer's runtime integer check instead
                // of acquiring a concrete integer type during candidate admission.
                let domains=if stage==StreamStageKind::Sum { vec![Some(Atom::Int),Some(Atom::UInt),Some(Atom::Any)] } else { vec![None] };
                for numeric in domains { for variant in variants(&stage,form) { candidates.push(self.import(graph,stage.clone(),source,numeric,form,variant,origin)?); } }
            }
            let family=graph.register_family(&candidates)?; self.families.insert(key,family); Ok(family)
        });
        if result.is_err() { self.families.retain(|_,id|graph.family(*id).is_ok()); self.candidates.retain(|id,_|graph.candidate(*id).is_ok()); } result
    }

    pub(crate) fn metadata(&self,graph:&InferenceContext,id:CandidateId)->Result<&StageCandidate,InferenceError> {
        if self.owner!=Some(graph.owner()) { return Err(InferenceError::ForeignHandle); }
        let template=graph.candidate(id)?; let metadata=self.candidates.get(&id).ok_or(InferenceError::ForeignHandle)?;
        if template.identity!=metadata.identity||template.scheme!=metadata.scheme||!template.has_receiver { return Err(InferenceError::InvalidScheme); } Ok(metadata)
    }

    pub(crate) fn retained_candidates(&self, graph: &InferenceContext, candidates: &[CandidateId]) -> Result<Vec<(CandidateId, StageCandidate)>, InferenceError> {
        if self.owner.is_some_and(|owner| owner != graph.owner()) { return Err(InferenceError::ForeignHandle); }
        let mut retained = Vec::new();
        for &candidate in candidates {
            graph.candidate(candidate)?;
            if self.candidates.contains_key(&candidate) { retained.push((candidate, self.metadata(graph, candidate)?.clone())); }
        }
        Ok(retained)
    }

    pub(crate) fn output_effect_bindings(&self,graph:&mut InferenceContext,family:OperationFamilyId,level:u32)->Result<Vec<(ProducerRole,EffectSummary)>,InferenceError> {
        let candidates=graph.family(family)?.to_vec();
        let first=*candidates.first().ok_or(InferenceError::InvalidScheme)?; let terminal=is_terminal(&self.metadata(graph,first)?.stage);
        for candidate in candidates { if is_terminal(&self.metadata(graph,candidate)?.stage)!=terminal { return Err(InferenceError::InvalidScheme); } }
        if terminal { return Ok(Vec::new()); }
        Ok(vec![(ProducerRole::Pull,EffectSummary::Variable(graph.fresh_derived_effect_at(level,None)?)),(ProducerRole::Close,EffectSummary::Variable(graph.fresh_derived_effect_at(level,None)?))])
    }

    /// Pending candidates retain output roots without assigning a producer
    /// domain. Selection ties them to one instantiated composition exactly once.
    pub(crate) fn output_producer_effects(&self,graph:&InferenceContext,requirement:RequirementId)->Result<Option<(EffectSummary,EffectSummary)>,InferenceError> {
        let RequirementTemplate::Operation { family,call }=graph.requirement_template(requirement)? else { return Err(InferenceError::InvalidScheme) };
        for candidate in graph.family(family)? { self.metadata(graph,*candidate)?; }
        let outputs=&graph.operation_call(call)?.output_effect_bindings;
        if outputs.is_empty() { return Ok(None); }
        let pull=outputs.iter().find_map(|(role,root)|(*role==ProducerRole::Pull).then_some(*root)).ok_or(InferenceError::InvalidScheme)?;
        let close=outputs.iter().find_map(|(role,root)|(*role==ProducerRole::Close).then_some(*root)).ok_or(InferenceError::InvalidScheme)?;
        if outputs.len()!=2 { return Err(InferenceError::InvalidScheme); } Ok(Some((pull,close)))
    }

    fn import(&mut self,graph:&mut InferenceContext,stage:StreamStageKind,source:StageSource,input_numeric:Option<Atom>,form:StageForm,variant:StageVariant,origin:Span)->Result<CandidateId,InferenceError> {
        let reason=graph.reason(origin,None)?; let mut requirements=Vec::new();
        let item=graph.fresh(1,origin)?; let mapped=graph.fresh(1,origin)?; let key=graph.fresh(1,origin)?; let accumulator=graph.fresh(1,origin)?;
        if let Some(numeric)=input_numeric { let numeric=graph.atom(numeric)?; graph.unify(item,numeric,reason)?; }
        let creation=EffectSummary::Variable(graph.fresh_derived_effect_at(1,None)?);
        let mut roles=vec![(EffectRole::Creation,creation)];
        let mut pull_roles=Vec::new(); let mut close_roles=Vec::new();
        let receiver=match source {
            StageSource::Str=>graph.atom(Atom::Str)?,StageSource::Bytes=>graph.atom(Atom::Bytes)?,
            StageSource::Sequence { domain,outer_result }=>{
                add_sequence_producer(graph,domain,outer_result,0,&mut roles,&mut pull_roles,&mut close_roles)?;
                let receiver=sequence(graph,domain,item)?;
                if outer_result { let error=graph.fresh(1,origin)?; graph.include_effects(EffectSummary::Closed(EffectSet::ERROR),creation,reason)?; graph.result(receiver,error)? } else { receiver }
            }
        };
        let mut parameters=vec![Parameter { label:Name::intern("<pipeline>"),ty:receiver,defaulted:false,rest:false }];
        let config=stage_parameters(stage.as_str());
        let mut additional_producer=None;
        for parameter in config {
            let ty=match parameter.ty {
                StageParameterType::Int=>graph.atom(Atom::Int)?,StageParameterType::Bool=>graph.atom(Atom::Bool)?,StageParameterType::Value=>accumulator,
                StageParameterType::Columns=>{ let string=graph.atom(Atom::Str)?; graph.list(string)? },
                StageParameterType::Sequence=>{
                    let domain=variant.other_sequence.ok_or(InferenceError::InvalidScheme)?;
                    additional_producer=Some(AdditionalProducer::ZipArgument); add_sequence_producer(graph,domain,false,1,&mut roles,&mut pull_roles,&mut close_roles)?;
                    sequence(graph,domain,mapped)?
                }
            };
            parameters.push(Parameter { label:Name::intern(parameter.name),ty,defaulted:parameter.default!=StageParameterDefault::Required,rest:false });
        }
        let mut callback_slot=None; let mut intrinsic_pull=EffectSet::EMPTY;
        let callback_result=if form.callback_kind.is_some() {
            let result=match stage {
                StreamStageKind::Where|StreamStageKind::Any|StreamStageKind::All=>graph.atom(Atom::Bool)?,
                StreamStageKind::Each|StreamStageKind::Tee=>{ let unit=graph.atom(Atom::Unit)?; if variant.callback_shape==CallbackShape::ResultUnit { let error=graph.fresh(1,origin)?; intrinsic_pull=EffectSet::ERROR; graph.result(unit,error)? } else { unit } },
                StreamStageKind::FlatMap=>{
                    let domain=if matches!(variant.callback_shape,CallbackShape::List|CallbackShape::ResultList) { SequenceDomain::List } else { SequenceDomain::Stream };
                    additional_producer=Some(AdditionalProducer::CallbackResult); add_sequence_producer(graph,domain,matches!(variant.callback_shape,CallbackShape::ResultList|CallbackShape::ResultStream),1,&mut roles,&mut pull_roles,&mut close_roles)?;
                    let sequence=sequence(graph,domain,mapped)?;
                    if matches!(variant.callback_shape,CallbackShape::ResultList|CallbackShape::ResultStream) { let error=graph.fresh(1,origin)?; intrinsic_pull=EffectSet::ERROR; graph.result(sequence,error)? } else { sequence }
                }
                StreamStageKind::Fold|StreamStageKind::Reduce=>accumulator,
                StreamStageKind::ReduceBy=>{
                    let value=match variant.reduction_shape { ReductionShape::Int=>graph.atom(Atom::Int)?,ReductionShape::UInt=>graph.atom(Atom::UInt)?,ReductionShape::Float=>graph.atom(Atom::Float)?,ReductionShape::Record=>{ let tail=graph.fresh_row(1,origin)?; let row=graph.row(Vec::new(),Some(tail))?; graph.record(row)? },ReductionShape::Value=>mapped };
                    graph.unify(mapped,value,reason)?;
                    require(graph,&mut requirements,Eligibility::CountKey,key,reason)?;
                    { let tail=graph.fresh_row(1,origin)?; let row=graph.row(vec![RowField { label:Name::intern("key"),ty:key },RowField { label:Name::intern("value"),ty:value }],Some(tail))?; graph.record(row)? }
                }
                StreamStageKind::SortBy|StreamStageKind::UniqueBy|StreamStageKind::GroupBy|StreamStageKind::Count=>key,
                _=>mapped,
            };
            let effects=EffectSummary::Variable(graph.fresh_effect_at(1,None)?); roles.push((EffectRole::Callback,effects)); pull_roles.push(EffectRole::Callback);
            let parameters=if matches!(stage,StreamStageKind::Fold|StreamStageKind::Reduce) { vec![Parameter { label:Name::intern("<acc>"),ty:accumulator,defaulted:false,rest:false },Parameter { label:Name::intern("<item>"),ty:item,defaulted:false,rest:false }] } else { vec![Parameter { label:Name::intern("<item>"),ty:item,defaulted:false,rest:false }] };
            let callback=graph.arrow(Arrow { kind:form.callback_kind.ok_or(InferenceError::InvalidScheme)?,params:parameters,result,effects })?;
            callback_slot=Some(config.len());
            Some((callback,result))
        } else { None };
        if let Some((callback,_))=callback_result { parameters.push(Parameter { label:Name::intern("<callback>"),ty:callback,defaulted:false,rest:false }); }
        let result=match stage {
            StreamStageKind::Map|StreamStageKind::ParMap=>{ require(graph,&mut requirements,Eligibility::NonUnit,mapped,reason)?; graph.stream(mapped)? },
            StreamStageKind::Where|StreamStageKind::Take|StreamStageKind::Drop|StreamStageKind::UniqueBy|StreamStageKind::Repeat|StreamStageKind::Tee|StreamStageKind::Shuffle=>graph.stream(item)?,
            StreamStageKind::Batch=>{ if form.batch_argv { require(graph,&mut requirements,Eligibility::ArgvItem,item,reason)?; } let list=graph.list(item)?; graph.stream(list)? },
            StreamStageKind::Sort=>{ require(graph,&mut requirements,Eligibility::Sortable,item,reason)?; graph.stream(item)? },
            StreamStageKind::SortBy=>{ require(graph,&mut requirements,Eligibility::SortableKey,key,reason)?; graph.stream(item)? },
            StreamStageKind::First|StreamStageKind::Last|StreamStageKind::Min|StreamStageKind::Max=>{ let error=graph.atom(Atom::Error)?; graph.result(item,error)? },
            StreamStageKind::Each|StreamStageKind::TablePrint=>{ if stage==StreamStageKind::TablePrint { require(graph,&mut requirements,Eligibility::Record,item,reason)?; } graph.atom(Atom::Unit)? },
            StreamStageKind::Enumerate=>{ let index=graph.atom(Atom::Int)?; let row=record(graph,&[("index",index),("value",item)])?; graph.stream(row)? },
            StreamStageKind::Zip=>{ let row=record(graph,&[("left",item),("right",mapped)])?; graph.stream(row)? },
            StreamStageKind::Range=>{ let int=graph.atom(Atom::Int)?; graph.stream(int)? },
            StreamStageKind::Sum=>graph.atom(Atom::Int)?,
            StreamStageKind::GroupBy=>{ let items=graph.list(item)?; let row=record(graph,&[("key",key),("items",items)])?; graph.stream(row)? },
            StreamStageKind::Fold|StreamStageKind::Reduce=>accumulator,
            StreamStageKind::FlatMap=>graph.stream(mapped)?,
            StreamStageKind::Any|StreamStageKind::All=>graph.atom(Atom::Bool)?,
            StreamStageKind::Count=>{ let int=graph.atom(Atom::Int)?; if form.callback_kind.is_some() { require(graph,&mut requirements,Eligibility::CountKey,key,reason)?; let string=graph.atom(Atom::Str)?; graph.map(string,int)? } else { int } },
            StreamStageKind::Collect=>graph.list(item)?,
            StreamStageKind::ReduceBy=>{ let string=graph.atom(Atom::Str)?; graph.map(string,mapped)? },
            StreamStageKind::TextStreamLines=>{ let string=graph.atom(Atom::Str)?; graph.stream(string)? },
            StreamStageKind::BytesChunks=>{ let bytes=graph.atom(Atom::Bytes)?; graph.stream(bytes)? },
            StreamStageKind::JsonLines|StreamStageKind::JsonStream=>{ let any=graph.atom(Atom::Any)?; graph.stream(any)? },
        };
        pull_roles.extend(close_roles.iter().copied());
        if is_terminal(&stage) {
            graph.include_effects(EffectSummary::Closed(intrinsic_pull),creation,reason)?;
            for (role,effects) in &roles { if *role!=EffectRole::Creation { graph.include_effects(*effects,creation,reason)?; } }
        } else {
            let upper=if matches!(source,StageSource::Sequence { outer_result:true,.. }) { EffectSet::ERROR } else { EffectSet::EMPTY };
            graph.include_effects(creation,EffectSummary::Closed(upper),reason)?;
        }
        let mut relations=vec![ArgumentRelation::Assignable;parameters.len()]; relations[0]=ArgumentRelation::Exact;
        if let Some(slot)=callback_slot { relations[slot+1]=if form.callback_protocol { ArgumentRelation::InvocationProtocol } else if matches!(stage,StreamStageKind::Fold|StreamStageKind::Reduce) { ArgumentRelation::Assignable } else { ArgumentRelation::Exact }; }
        let arrow=graph.arrow(Arrow { kind:CallableKind::Pure,params:parameters,result,effects:creation })?;
        let mut effect_roots:Vec<_>=roles.iter().map(|(_,effects)|*effects).collect(); let mut output_effect_roles=Vec::new();
        if !is_terminal(&stage) {
            let pull=EffectSummary::Variable(graph.fresh_derived_effect_at(1,None)?); let close=EffectSummary::Variable(graph.fresh_derived_effect_at(1,None)?);
            graph.include_effects(EffectSummary::Closed(intrinsic_pull),pull,reason)?;
            for (selected,target) in [(&pull_roles,pull),(&close_roles,close)] { for selected in selected { let source=roles.iter().find_map(|(role,effects)|(*role==*selected).then_some(*effects)).ok_or(InferenceError::InvalidScheme)?; graph.include_effects(source,target,reason)?; } }
            output_effect_roles.push((ProducerRole::Pull,effect_roots.len() as u32)); effect_roots.push(pull);
            output_effect_roles.push((ProducerRole::Close,effect_roots.len() as u32)); effect_roots.push(close);
        }
        let scheme=graph.generalize_with_effect_roots(arrow,0,Generalization::Allowed,&requirements,&effect_roots)?;
        let effect_roles=roles.iter().zip(&graph.scheme(scheme)?.effect_roots).filter(|((role,_),_)|*role!=EffectRole::Creation).map(|((role,_),root)|match root { EffectSummary::Rigid { scope,index } if *scope==scheme=>Ok((*role,EffectRoleReference::Binder(*index))),EffectSummary::Closed(bits)=>Ok((*role,EffectRoleReference::Fixed(*bits))),_=>Err(InferenceError::InvalidScheme) }).collect::<Result<Vec<_>,_>>()?;
        let identity=Name::intern(&format!("language.stage.{}:{source:?}:{input_numeric:?}:{form:?}:{variant:?}",stage.as_str()));
        let id=graph.register_candidate(CandidateTemplate { failure_projection: None, output_effect_roles, identity,public_label:Name::intern(stage.as_str()),effect_roles,scheme,has_receiver:true,actual_eligibility:Vec::new(),argument_relations:relations })?;
        self.candidates.insert(id,StageCandidate { identity,scheme,stage,source,input_numeric,form,variant,parameters:config,callback_slot,additional_producer,effects:StageEffectRelation { output_pull:pull_roles.into(),output_close:close_roles.into(),intrinsic_pull } }); Ok(id)
    }
}
fn sequence(graph:&mut InferenceContext,domain:SequenceDomain,item:TypeId)->Result<TypeId,InferenceError> { match domain { SequenceDomain::List=>graph.list(item),SequenceDomain::Stream=>graph.stream(item) } }
fn record(graph:&mut InferenceContext,fields:&[(&str,TypeId)])->Result<TypeId,InferenceError> { let row=graph.row(fields.iter().map(|(label,ty)|RowField { label:Name::intern(label),ty:*ty }).collect(),None)?; graph.record(row) }
fn require(graph:&mut InferenceContext,requirements:&mut Vec<RequirementId>,predicate:Eligibility,ty:TypeId,reason:crate::sema::inference::ReasonId)->Result<(),InferenceError> { requirements.push(graph.require_eligibility(predicate,ty,reason)?); Ok(()) }
pub(crate) fn sequence_effect_roles(source:u32)->[EffectRole;4] {
    [EffectRole::Pull { source },EffectRole::Close { source },EffectRole::PullProjection { source,projection:EffectProjection::ResultSuccess },EffectRole::CloseProjection { source,projection:EffectProjection::ResultSuccess }]
}
fn add_sequence_producer(graph:&mut InferenceContext,domain:SequenceDomain,outer_result:bool,source:u32,roles:&mut Vec<(EffectRole,EffectSummary)>,pull:&mut Vec<EffectRole>,close:&mut Vec<EffectRole>)->Result<(),InferenceError> {
    let locations=sequence_effect_roles(source);
    for (index,role) in locations.into_iter().enumerate() {
        let projected=index>=2;
        let effect=if domain==SequenceDomain::Stream&&projected==outer_result { EffectSummary::Variable(graph.fresh_effect_at(1,None)?) } else { EffectSummary::Closed(EffectSet::EMPTY) };
        roles.push((role,effect));
        if projected==outer_result { if index%2==0 { pull.push(role); } else { close.push(role); } }
    }
    Ok(())
}
fn is_terminal(stage:&StreamStageKind)->bool { matches!(stage,StreamStageKind::Each|StreamStageKind::First|StreamStageKind::Last|StreamStageKind::Sum|StreamStageKind::Min|StreamStageKind::Max|StreamStageKind::Fold|StreamStageKind::Reduce|StreamStageKind::Any|StreamStageKind::All|StreamStageKind::TablePrint|StreamStageKind::Count|StreamStageKind::Collect|StreamStageKind::ReduceBy) }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StageCallbackPresence { Absent, Optional, Required }

pub(crate) fn stage_callback_presence(stage: &StreamStageKind) -> StageCallbackPresence {
    use StreamStageKind::*;
    match stage {
        Where | Map | ParMap | Each | SortBy | UniqueBy | Tee | GroupBy | Fold | Reduce | FlatMap | Any | All | ReduceBy => StageCallbackPresence::Required,
        Count => StageCallbackPresence::Optional,
        Batch | Sort | Take | Drop | First | Last | Enumerate | Zip | Range | Repeat | Sum | Min | Max | Shuffle | TablePrint | TextStreamLines | BytesChunks | JsonLines | JsonStream | Collect => StageCallbackPresence::Absent,
    }
}

fn validate_form(stage:&StreamStageKind,form:StageForm)->Result<(),InferenceError> {
    let presence = stage_callback_presence(stage);
    if presence != StageCallbackPresence::Optional && (presence == StageCallbackPresence::Required) != form.callback_kind.is_some() { return Err(InferenceError::Boundary("stage callback presence disagrees with its fixed contract")); }
    if form.callback_protocol && form.callback_kind.is_none() { return Err(InferenceError::Boundary("callback invocation protocol requires a callback form")); }
    if form.callback_kind==Some(CallableKind::Stream) { return Err(InferenceError::Boundary("stage callback requires a checked value callable")); }
    if form.batch_argv&&*stage!=StreamStageKind::Batch { return Err(InferenceError::Boundary("argv item constraint belongs to batch")); }
    if *stage==StreamStageKind::ReduceBy {
        let modes=form.reduction_modes.ok_or(InferenceError::Boundary("reduce-by requires checked possible reduction modes"))?;
        if modes.0==0||modes.0&!7!=0 { return Err(InferenceError::Boundary("invalid reduction mode mask")); }
    } else if form.reduction_modes.is_some() { return Err(InferenceError::Boundary("reduction modes belong to reduce-by")); }
    Ok(())
}

fn variants(stage:&StreamStageKind,form:StageForm)->Vec<StageVariant> {
    let shapes=if form.callback_kind.is_none() { vec![CallbackShape::Absent] } else if matches!(stage,StreamStageKind::Each|StreamStageKind::Tee) { vec![CallbackShape::Unit,CallbackShape::ResultUnit] } else if *stage==StreamStageKind::FlatMap { vec![CallbackShape::List,CallbackShape::Stream,CallbackShape::ResultList,CallbackShape::ResultStream] } else { vec![CallbackShape::Value] };
    let sequences=if *stage==StreamStageKind::Zip { vec![Some(SequenceDomain::List),Some(SequenceDomain::Stream)] } else { vec![None] };
    let reductions=if form.reduction_modes.is_some_and(ReductionModes::contains_sum) { vec![ReductionShape::Int,ReductionShape::UInt,ReductionShape::Float,ReductionShape::Record] } else { vec![ReductionShape::Value] };
    let mut variants=Vec::new();
    for callback_shape in shapes { for other_sequence in &sequences { for reduction_shape in &reductions { variants.push(StageVariant { callback_shape,other_sequence:*other_sequence,reduction_shape:*reduction_shape }); } } } variants
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceId;
    fn span()->Span { Span::new(SourceId::new(0),0,1) }
    #[test]
    fn lazy_map_retains_callback_effect_role_and_result_data() {
        use crate::sema::inference::{OperationCall,TypeNode};
        let symbols = crate::symbol::SymbolOwner::new(); let _symbols = symbols.enter();
        let mut graph = InferenceContext::default(); let mut catalog = StageGraph::default(); let why=graph.reason(span(),None).unwrap();
        let form = StageForm { callback_kind: Some(CallableKind::Proc), ..StageForm::default() };
        let family=catalog.family(&mut graph,StreamStageKind::Map,form,span()).unwrap(); let outputs=catalog.output_effect_bindings(&mut graph,family,1).unwrap();
        let int=graph.atom(Atom::Int).unwrap(); let error=graph.atom(Atom::Error).unwrap(); let data=graph.result(int,error).unwrap(); let source=graph.stream(int).unwrap(); let result=graph.stream(data).unwrap();
        let callback=graph.arrow(Arrow { kind:CallableKind::Proc,params:vec![Parameter { label:Name::intern("<item>"),ty:int,defaulted:false,rest:false }],result:data,effects:EffectSummary::Closed(EffectSet::FS) }).unwrap();
        let requirement=graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:outputs.clone(), receiver:Some(source),arguments:vec![Some(callback)],result,effects:EffectSummary::Closed(EffectSet::EMPTY),effect_bindings:vec![(EffectRole::Callback,EffectSummary::Closed(EffectSet::FS)),(EffectRole::Pull { source:0 },EffectSummary::Closed(EffectSet::NET)),(EffectRole::Close { source:0 },EffectSummary::Closed(EffectSet::PROCESS)),(EffectRole::PullProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::CloseProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY))] },why).unwrap();
        graph.solve().unwrap(); let evidence=graph.candidate_evidence(requirement).unwrap().unwrap().clone();
        assert!(matches!(graph.node(graph.resolved(result).unwrap()).unwrap(),TypeNode::Stream(item) if *item==data));
        let metadata=catalog.metadata(&graph,evidence.candidate).unwrap(); assert_eq!(metadata.stage,StreamStageKind::Map); assert_eq!(metadata.callback_slot,Some(0));
        let (pull,close)=catalog.output_producer_effects(&graph,requirement).unwrap().unwrap(); graph.solve().unwrap();
        let EffectSummary::Variable(pull)=pull else { panic!() }; let EffectSummary::Variable(close)=close else { panic!() };
        assert_eq!(graph.effect_value(pull).unwrap(),EffectSet(EffectSet::NET.0|EffectSet::FS.0|EffectSet::PROCESS.0));
        assert_eq!(graph.effect_value(close).unwrap(),EffectSet::PROCESS);
    }    fn all_stages()->Vec<StreamStageKind> { use StreamStageKind::*; vec![Where,Map,ParMap,Each,Batch,Sort,SortBy,Take,Drop,First,Last,UniqueBy,Enumerate,Zip,Range,Repeat,Tee,Sum,Min,Max,GroupBy,Fold,Reduce,FlatMap,Any,All,Shuffle,TablePrint,TextStreamLines,BytesChunks,JsonLines,JsonStream,Count,Collect,ReduceBy] }
    fn ordinary_form(stage:&StreamStageKind)->StageForm {
        let mut form=StageForm::default();
        if matches!(stage,StreamStageKind::Where|StreamStageKind::Map|StreamStageKind::ParMap|StreamStageKind::Each|StreamStageKind::SortBy|StreamStageKind::UniqueBy|StreamStageKind::Tee|StreamStageKind::GroupBy|StreamStageKind::Fold|StreamStageKind::Reduce|StreamStageKind::FlatMap|StreamStageKind::Any|StreamStageKind::All|StreamStageKind::ReduceBy) { form.callback_kind=Some(CallableKind::Pure); }
        if *stage==StreamStageKind::ReduceBy { form.reduction_modes=Some(ReductionModes::SUM); }
        form
    }
    #[test]
    fn stage_catalog_reconciles_all_frozen_stages_and_configuration_owners() {
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter();
        let inventory=crate::modules::json::parse_raw_json(include_str!("../../bench/typing/operations.json")).unwrap();
        let miniserde::json::Value::Object(inventory)=inventory else { panic!() }; let miniserde::json::Value::Array(entries)=inventory.get("operations").unwrap() else { panic!() };
        let expected:std::collections::BTreeSet<_>=entries.iter().filter_map(|entry| { let miniserde::json::Value::Object(entry)=entry else { return None }; let miniserde::json::Value::String(id)=entry.get("id")? else { return None }; id.starts_with("language.stage.").then_some(id.clone()) }).collect();
        let observed:std::collections::BTreeSet<_>=all_stages().iter().map(|stage|format!("language.stage.{}",stage.as_str())).collect(); assert_eq!(observed.len(),35); assert_eq!(observed,expected);
        let mut graph=InferenceContext::default(); let mut catalog=StageGraph::default();
        for stage in all_stages() { let family=catalog.family(&mut graph,stage.clone(),ordinary_form(&stage),span()).unwrap();
            for candidate in graph.family(family).unwrap() { let metadata=catalog.metadata(&graph,*candidate).unwrap(); assert_eq!(metadata.stage,stage); assert_eq!(metadata.parameters,stage_parameters(stage.as_str())); }
        }
    }

    #[test]
    fn every_stage_candidate_proves_required_and_defaulted_slots_and_rejects_extra_arguments() {
        use crate::sema::inference::{OperationCall,RequirementTemplate,TypeNode,VariableKind};
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter(); let mut graph=InferenceContext::default(); let mut catalog=StageGraph::default();
        for stage in all_stages() {
            let form=ordinary_form(&stage); catalog.family(&mut graph,stage.clone(),form,span()).unwrap();
            if form.callback_kind.is_some() { let mut proc=form; proc.callback_kind=Some(CallableKind::Proc); catalog.family(&mut graph,stage.clone(),proc,span()).unwrap(); }
            if stage==StreamStageKind::Count { let callback=StageForm { callback_kind:Some(CallableKind::Pure),..form }; catalog.family(&mut graph,stage.clone(),callback,span()).unwrap(); }
            if stage==StreamStageKind::Batch { let mut bounded=form; bounded.batch_argv=true; catalog.family(&mut graph,stage.clone(),bounded,span()).unwrap(); }
            if stage==StreamStageKind::ReduceBy {
                for modes in [ReductionModes::MIN,ReductionModes::MAX,ReductionModes::ALL] { let mut variant=form; variant.reduction_modes=Some(modes); catalog.family(&mut graph,stage.clone(),variant,span()).unwrap(); }
            }
        }
        assert!(catalog.candidates.len()>200);
        let candidates:Vec<_>=catalog.candidates.keys().copied().collect(); let why=graph.reason(span(),None).unwrap(); let string=graph.atom(Atom::Str).unwrap(); let ground_record=record(&mut graph,&[("value",string)]).unwrap();
        for candidate in candidates {
            let template=graph.candidate(candidate).unwrap().clone(); let instance=graph.instantiate(template.scheme,1,why).unwrap();
            for requirement in &instance.requirements {
                if let RequirementTemplate::Eligibility { predicate:Eligibility::Record,ty }=graph.requirement_template(*requirement).unwrap() { graph.unify(ty,ground_record,why).unwrap(); }
            }
            for substitution in &instance.substitutions {
                let Some(variable)=graph.variable(*substitution).unwrap() else { continue }; let kind=variable.kind;
                if kind==VariableKind::Row { let row=graph.row(Vec::new(),Some(*substitution)).unwrap(); let wrapper=graph.record(row).unwrap(); let empty=record(&mut graph,&[]).unwrap(); graph.unify(wrapper,empty,why).unwrap(); }
                else { graph.unify(*substitution,string,why).unwrap(); }
            }
            let TypeNode::Arrow(arrow)=graph.node(graph.resolved(instance.ty).unwrap()).unwrap() else { panic!() }; let arrow=arrow.clone();
            let roles=template.effect_roles.iter().map(|(role,reference)|(*role,EffectSummary::Closed(match reference { EffectRoleReference::Binder(index)=>graph.scheme(template.scheme).unwrap().effect_quantifiers[*index as usize].lower,EffectRoleReference::Fixed(bits)=>*bits }))).collect::<Vec<_>>();
            let arguments:Vec<_>=arrow.params[1..].iter().map(|parameter|Some(parameter.ty)).collect(); let family=graph.register_family(&[candidate]).unwrap(); let outputs=catalog.output_effect_bindings(&mut graph,family,1).unwrap();
            let valid=OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:outputs.clone(), receiver:Some(arrow.params[0].ty),arguments:arguments.clone(),result:arrow.result,effects:EffectSummary::Closed(EffectSet(127)),effect_bindings:roles.clone() };
            let requirement=graph.require_operation(family,valid,why).unwrap(); graph.solve().unwrap_or_else(|error|panic!("{}: {error:?}",template.identity.as_str())); assert_eq!(graph.candidate_evidence(requirement).unwrap().unwrap().candidate,candidate);
            for (index,parameter) in arrow.params[1..].iter().enumerate() {
                let outcome=graph.trial(|graph| { let mut supplied=arguments.clone(); supplied[index]=None; let requirement=graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:outputs.clone(), receiver:Some(arrow.params[0].ty),arguments:supplied,result:arrow.result,effects:EffectSummary::Closed(EffectSet(127)),effect_bindings:roles.clone() },why)?; graph.solve()?; if graph.candidate_evidence(requirement)?.is_none() { return Err(InferenceError::Boundary("stage remains ambiguous")); } Ok(()) });
                assert_eq!(outcome.is_ok(),parameter.defaulted,"{} slot{}: {outcome:?}",template.identity.as_str(),index);
            }
            assert!(graph.trial(|graph| { let mut arguments=arguments.clone(); arguments.push(Some(string)); graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:outputs.clone(), receiver:Some(arrow.params[0].ty),arguments,result:arrow.result,effects:EffectSummary::Closed(EffectSet(127)),effect_bindings:roles.clone() },why)?; graph.solve() }).is_err());
            assert!(graph.trial(|graph| { let mut missing=roles.clone(); if missing.pop().is_none() { missing.push((EffectRole::Callback,EffectSummary::Closed(EffectSet::EMPTY))); } graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:outputs.clone(), receiver:Some(arrow.params[0].ty),arguments:arguments.clone(),result:arrow.result,effects:EffectSummary::Closed(EffectSet(127)),effect_bindings:missing },why)?; graph.solve() }).is_err());
            assert!(graph.trial(|graph| { let mut missing=outputs.clone(); if missing.pop().is_none() { missing.push((ProducerRole::Pull,EffectSummary::Closed(EffectSet::EMPTY))); } graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:missing,receiver:Some(arrow.params[0].ty),arguments:arguments.clone(),result:arrow.result,effects:EffectSummary::Closed(EffectSet(127)),effect_bindings:roles.clone() },why)?; graph.solve() }).is_err());
        }
    }

    #[test]
    fn sum_preserves_integer_and_dynamic_domains_without_concrete_widening() {
        use crate::sema::inference::OperationCall;
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter(); let mut graph=InferenceContext::default(); let mut catalog=StageGraph::default(); let why=graph.reason(span(),None).unwrap();
        let family=catalog.family(&mut graph,StreamStageKind::Sum,StageForm::default(),span()).unwrap(); let result=graph.atom(Atom::Int).unwrap();
        for atom in [Atom::Int,Atom::UInt,Atom::Any,Atom::Float,Atom::Str] {
            let item=graph.atom(atom).unwrap(); let source=graph.stream(item).unwrap();
            let outcome=graph.trial(|graph| { let requirement=graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:Vec::new(), receiver:Some(source),arguments:Vec::new(),result,effects:EffectSummary::Closed(EffectSet::EMPTY),effect_bindings:vec![(EffectRole::Pull { source:0 },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::Close { source:0 },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::PullProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::CloseProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY))] },why)?; graph.solve()?; if graph.candidate_evidence(requirement)?.is_none() { return Err(InferenceError::Boundary("sum remains ambiguous")); } Ok(()) });
            assert_eq!(outcome.is_ok(),matches!(atom,Atom::Int|Atom::UInt|Atom::Any),"{atom:?}: {outcome:?}");
        }
        graph.trial(|graph| {
            let item = graph.fresh(1, span())?;
            let source = graph.stream(item)?;
            let requirement = graph.require_operation(family, OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings: Vec::new(), receiver: Some(source), arguments: Vec::new(), result, effects: EffectSummary::Closed(EffectSet::EMPTY), effect_bindings: vec![(EffectRole::Pull { source: 0 }, EffectSummary::Closed(EffectSet::EMPTY)), (EffectRole::Close { source: 0 }, EffectSummary::Closed(EffectSet::EMPTY)), (EffectRole::PullProjection { source: 0, projection: EffectProjection::ResultSuccess }, EffectSummary::Closed(EffectSet::EMPTY)), (EffectRole::CloseProjection { source: 0, projection: EffectProjection::ResultSuccess }, EffectSummary::Closed(EffectSet::EMPTY))] }, why)?;
            graph.solve()?;
            assert!(graph.candidate_evidence(requirement)?.is_none());
            assert!(matches!(graph.node(graph.resolved(item)?)?, crate::sema::inference::TypeNode::Meta(_)));
            Ok(())
        }).unwrap();
    }

    #[test]
    fn flat_map_preserves_inner_producer_roles_separately_from_callback_invocation() {
        use crate::sema::inference::OperationCall;
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter(); let mut graph=InferenceContext::default(); let mut catalog=StageGraph::default(); let why=graph.reason(span(),None).unwrap();
        let form=StageForm { callback_kind:Some(CallableKind::Proc),..StageForm::default() }; let family=catalog.family(&mut graph,StreamStageKind::FlatMap,form,span()).unwrap(); let outputs=catalog.output_effect_bindings(&mut graph,family,1).unwrap();
        let int=graph.atom(Atom::Int).unwrap(); let boolean=graph.atom(Atom::Bool).unwrap(); let error=graph.atom(Atom::Error).unwrap(); let source=graph.stream(int).unwrap(); let inner=graph.stream(boolean).unwrap(); let callback_result=graph.result(inner,error).unwrap();
        let callback=graph.arrow(Arrow { kind:CallableKind::Proc,params:vec![Parameter { label:Name::intern("<item>"),ty:int,defaulted:false,rest:false }],result:callback_result,effects:EffectSummary::Closed(EffectSet::FS) }).unwrap();
        let roles=vec![(EffectRole::Callback,EffectSummary::Closed(EffectSet::FS)),(EffectRole::Pull { source:0 },EffectSummary::Closed(EffectSet::NET)),(EffectRole::Close { source:0 },EffectSummary::Closed(EffectSet::PROCESS)),(EffectRole::PullProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::CloseProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::Pull { source:1 },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::Close { source:1 },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::PullProjection { source:1,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::TIME)),(EffectRole::CloseProjection { source:1,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::ENV))];
        let requirement=graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:outputs.clone(), receiver:Some(source),arguments:vec![Some(callback)],result:inner,effects:EffectSummary::Closed(EffectSet::EMPTY),effect_bindings:roles },why).unwrap(); graph.solve().unwrap();
        let evidence=graph.candidate_evidence(requirement).unwrap().unwrap(); assert_eq!(catalog.metadata(&graph,evidence.candidate).unwrap().additional_producer,Some(AdditionalProducer::CallbackResult));
        let (pull,close)=catalog.output_producer_effects(&graph,requirement).unwrap().unwrap(); let EffectSummary::Variable(pull)=pull else { panic!() }; let EffectSummary::Variable(close)=close else { panic!() };
        assert_eq!(graph.effect_value(pull).unwrap(),EffectSet(EffectSet::NET.0|EffectSet::FS.0|EffectSet::TIME.0|EffectSet::ERROR.0|EffectSet::PROCESS.0|EffectSet::ENV.0)); assert_eq!(graph.effect_value(close).unwrap(),EffectSet(EffectSet::PROCESS.0|EffectSet::ENV.0));
    }

    #[test]
    fn flat_map_output_effect_roots_exist_before_callback_result_selection() {
        use crate::sema::inference::OperationCall;
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter();
        let mut graph=InferenceContext::default(); let mut catalog=StageGraph::default(); let why=graph.reason(span(),None).unwrap();
        let family=catalog.family(&mut graph,StreamStageKind::FlatMap,StageForm { callback_kind:Some(CallableKind::Proc),..StageForm::default() },span()).unwrap(); let outputs=catalog.output_effect_bindings(&mut graph,family,1).unwrap();
        let int=graph.atom(Atom::Int).unwrap(); let boolean=graph.atom(Atom::Bool).unwrap(); let source=graph.stream(int).unwrap(); let result=graph.stream(boolean).unwrap();
        let callback_result=graph.fresh(1,span()).unwrap();
        let callback=graph.arrow(Arrow { kind:CallableKind::Proc,params:vec![Parameter { label:Name::intern("<item>"),ty:int,defaulted:false,rest:false }],result:callback_result,effects:EffectSummary::Closed(EffectSet::FS) }).unwrap();
        let roles=vec![(EffectRole::Callback,EffectSummary::Closed(EffectSet::FS)),(EffectRole::Pull { source:0 },EffectSummary::Closed(EffectSet::NET)),(EffectRole::Close { source:0 },EffectSummary::Closed(EffectSet::PROCESS)),(EffectRole::PullProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::CloseProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::Pull { source:1 },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::Close { source:1 },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::PullProjection { source:1,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::CloseProjection { source:1,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY))];
        let requirement=graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:outputs.clone(), receiver:Some(source),arguments:vec![Some(callback)],result,effects:EffectSummary::Closed(EffectSet::EMPTY),effect_bindings:roles },why).unwrap();
        graph.solve().unwrap(); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
        let (pull,close)=catalog.output_producer_effects(&graph,requirement).expect("pending stage must retain its producer output roots").unwrap();
        let inner=graph.stream(boolean).unwrap(); let error=graph.atom(Atom::Error).unwrap(); let returned=graph.result(inner,error).unwrap();
        graph.unify(callback_result,returned,why).unwrap(); graph.solve().unwrap();
        assert!(graph.candidate_evidence(requirement).unwrap().is_some());
        let EffectSummary::Variable(pull)=pull else { panic!() }; let EffectSummary::Variable(close)=close else { panic!() };
        assert_eq!(graph.effect_value(pull).unwrap(),EffectSet(EffectSet::FS.0|EffectSet::NET.0|EffectSet::PROCESS.0|EffectSet::ERROR.0));
        assert_eq!(graph.effect_value(close).unwrap(),EffectSet::PROCESS);
    }

    #[test]
    fn generalized_flat_map_output_roots_keep_conditional_error_independent_per_call() {
        use crate::sema::inference::{OperationCall,TypeNode};
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter(); let mut graph=InferenceContext::default(); let mut catalog=StageGraph::default(); let why=graph.reason(span(),None).unwrap();
        let family=catalog.family(&mut graph,StreamStageKind::FlatMap,StageForm { callback_kind:Some(CallableKind::Proc),..StageForm::default() },span()).unwrap();
        let outputs=catalog.output_effect_bindings(&mut graph,family,1).unwrap(); let int=graph.atom(Atom::Int).unwrap(); let boolean=graph.atom(Atom::Bool).unwrap(); let source=graph.stream(int).unwrap(); let result=graph.stream(boolean).unwrap(); let callback_result=graph.fresh(1,span()).unwrap();
        let callback=graph.arrow(Arrow { kind:CallableKind::Proc,params:vec![Parameter { label:Name::intern("<item>"),ty:int,defaulted:false,rest:false }],result:callback_result,effects:EffectSummary::Closed(EffectSet::FS) }).unwrap();
        let requirement=graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, output_effect_bindings:outputs.clone(),receiver:Some(source),arguments:vec![Some(callback)],result,effects:EffectSummary::Closed(EffectSet::EMPTY),effect_bindings:vec![(EffectRole::Callback,EffectSummary::Closed(EffectSet::FS)),(EffectRole::Pull { source:0 },EffectSummary::Closed(EffectSet::NET)),(EffectRole::Close { source:0 },EffectSummary::Closed(EffectSet::PROCESS)),(EffectRole::PullProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::CloseProjection { source:0,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::Pull { source:1 },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::Close { source:1 },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::PullProjection { source:1,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY)),(EffectRole::CloseProjection { source:1,projection:EffectProjection::ResultSuccess },EffectSummary::Closed(EffectSet::EMPTY))] },why).unwrap();
        graph.solve().unwrap(); assert!(graph.candidate_evidence(requirement).unwrap().is_none());
        let body=graph.arrow(Arrow { kind:CallableKind::Pure,params:vec![Parameter { label:Name::intern("callback"),ty:callback,defaulted:false,rest:false }],result,effects:EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let roots:Vec<_>=outputs.iter().map(|(_,root)|*root).collect(); let scheme=graph.generalize_with_effect_roots(body,0,Generalization::Allowed,&[requirement],&roots).unwrap();
        let first=graph.instantiate(scheme,1,why).unwrap(); let second=graph.instantiate(scheme,1,why).unwrap();
        assert_ne!(first.effect_roots[0],second.effect_roots[0]);
        for (instance,lifted) in [(first,false),(second,true)] {
            let TypeNode::Arrow(body)=graph.node(graph.resolved(instance.ty).unwrap()).unwrap() else { panic!() }; let callback=body.params[0].ty;
            let TypeNode::Arrow(callback)=graph.node(graph.resolved(callback).unwrap()).unwrap() else { panic!() }; let callback_result=callback.result;
            let returned=if lifted { let inner=graph.stream(boolean).unwrap(); let error=graph.atom(Atom::Error).unwrap(); graph.result(inner,error).unwrap() } else { graph.list(boolean).unwrap() };
            graph.unify(callback_result,returned,why).unwrap(); graph.solve().unwrap();
            assert!(instance.requirements.iter().any(|requirement|graph.candidate_evidence(*requirement).unwrap().is_some()));
            let expected=EffectSet(EffectSet::FS.0|EffectSet::NET.0|EffectSet::PROCESS.0|if lifted {EffectSet::ERROR.0} else {0});
            assert_eq!(graph.closed_effect_summary(instance.effect_roots[0]).unwrap(),EffectSummary::Closed(expected));
            assert_eq!(graph.closed_effect_summary(instance.effect_roots[1]).unwrap(),EffectSummary::Closed(EffectSet::PROCESS));
        }
    }

    #[test]
    fn result_stream_source_retains_only_its_projected_pull_and_cleanup() {
        use crate::sema::inference::OperationCall;
        let symbols=crate::symbol::SymbolOwner::new(); let _symbols=symbols.enter();
        let mut graph=InferenceContext::default(); let mut catalog=StageGraph::default(); let why=graph.reason(span(),None).unwrap();
        let family=catalog.family(&mut graph,StreamStageKind::Collect,StageForm::default(),span()).unwrap();
        let int=graph.atom(Atom::Int).unwrap(); let error=graph.atom(Atom::Error).unwrap(); let stream=graph.stream(int).unwrap(); let source=graph.result(stream,error).unwrap(); let result=graph.list(int).unwrap();
        let roles=sequence_effect_roles(0);
        let bindings=vec![(roles[0],EffectSummary::Closed(EffectSet::EMPTY)),(roles[1],EffectSummary::Closed(EffectSet::EMPTY)),(roles[2],EffectSummary::Closed(EffectSet::TIME)),(roles[3],EffectSummary::Closed(EffectSet::ENV))];
        let effects=EffectSummary::Variable(graph.fresh_derived_effect_at(1,None).unwrap());
        let requirement=graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver:Some(source),arguments:Vec::new(),result,effects,effect_bindings:bindings.clone(),output_effect_bindings:Vec::new() },why).unwrap();
        graph.solve().unwrap(); graph.seal_derived_effects(&[effects]).unwrap(); graph.solve().unwrap();
        assert_eq!(graph.closed_effect_summary(effects).unwrap(),EffectSummary::Closed(EffectSet(EffectSet::TIME.0|EffectSet::ENV.0|EffectSet::ERROR.0)));
        let evidence=graph.candidate_evidence(requirement).unwrap().unwrap(); let metadata=catalog.metadata(&graph,evidence.candidate).unwrap();
        assert_eq!(metadata.source,StageSource::Sequence { domain:SequenceDomain::Stream,outer_result:true });
        assert!(metadata.effects.output_pull.iter().all(|role|*role==roles[2]||*role==roles[3]));
        assert_eq!(metadata.effects.output_close.as_ref(),[roles[3]]);
        assert!(graph.trial(|graph| {
            let mut bindings=bindings; bindings[0].1=EffectSummary::Closed(EffectSet::NET);
            let effects=EffectSummary::Variable(graph.fresh_derived_effect_at(1,None)?);
            graph.require_operation(family,OperationCall { binding: crate::sema::inference::OperationBinding::Slots, effect_mode: crate::sema::inference::OperationEffectMode::AvailableBudget, mono_authority: None, declared_error_bound: None, receiver:Some(source),arguments:Vec::new(),result,effects,effect_bindings:bindings,output_effect_bindings:Vec::new() },why)?; graph.solve()
        }).is_err());
        for &candidate in graph.family(family).unwrap() {
            let scheme = graph.candidate(candidate).unwrap().scheme;
            graph.validate_scoped(crate::sema::inference::ScopedRoot { ty: graph.scheme(scheme).unwrap().body, scope: Some(scheme) }).unwrap();
        }
    }

}
