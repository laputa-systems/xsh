use super::*;
use super::super::semantic::{PairTypeKind, SemanticPoolBuilder, UnaryTypeKind};
use rustc_hash::FxHashMap;

// The memo belongs to one substitution bundle. Reusing it across distinct
// instances would let one caller's concrete signature train another caller.
struct Materialization {
    memo: FxHashMap<TypeRef, GroundTypeId>,
    active: Vec<TypeTemplateId>,
    work: usize,
}

impl Materialization {
    fn charge(&mut self, units: usize) -> Result<(), IrVerifyError> {
        self.work = self.work.checked_add(units).filter(|&work| work <= 2_000_000).ok_or_else(|| failure("type materialization exceeds its work bound"))?;
        Ok(())
    }

    fn materialize(&mut self, store: &GenericEvidenceStore, pools: &mut SemanticPools, semantic: &mut SemanticPoolBuilder, reference: TypeRef, substitutions: &[GroundTypeId]) -> Result<GroundTypeId, IrVerifyError> {
        self.charge(1)?;
        if let Some(&ty) = self.memo.get(&reference) { return Ok(ty); }
        let build = |result: Result<GroundTypeId, super::super::IrBuildError>| result.map_err(|_| failure("instantiated template cannot enter the ground pool"));
        let ty = match reference {
            TypeRef::Ground(ty) => { pools.type_tag(ty)?; ty }
            TypeRef::Rigid(index) => {
                let ty = *substitutions.get(index as usize).ok_or_else(|| failure("template substitution is out of scope"))?;
                pools.type_tag(ty)?;
                ty
            }
            TypeRef::Template(id) => {
                if self.active.len() >= 256 || self.active.contains(&id) { return Err(failure("type materialization is cyclic or exceeds its depth bound")); }
                self.active.push(id);
                let ty = match store.template(id)? {
                    TypeTemplate::Optional(inner) | TypeTemplate::List(inner) | TypeTemplate::Stream(inner) => {
                        let kind = match store.template(id)? { TypeTemplate::Optional(_) => UnaryTypeKind::Optional, TypeTemplate::List(_) => UnaryTypeKind::List, _ => UnaryTypeKind::Stream };
                        let child = self.materialize(store, pools, semantic, *inner, substitutions)?;
                        build(semantic.intern_unary_type(pools, kind, child))?
                    }
                    TypeTemplate::Map { key: left, value: right } | TypeTemplate::Result { ok: left, error: right } => {
                        let kind = if matches!(store.template(id)?, TypeTemplate::Map { .. }) { PairTypeKind::Map } else { PairTypeKind::Result };
                        let left = self.materialize(store, pools, semantic, *left, substitutions)?;
                        let right = self.materialize(store, pools, semantic, *right, substitutions)?;
                        build(semantic.intern_pair_type(pools, kind, left, right))?
                    }
                    TypeTemplate::Record { fields, row_tail } => {
                        self.charge(fields.len())?;
                        let mut row = std::collections::BTreeMap::new();
                        for &(name, reference) in fields {
                            let ty = self.materialize(store, pools, semantic, reference, substitutions)?;
                            if row.insert(name, ty).is_some() { return Err(failure("record template contains duplicate fields")); }
                        }
                        if let Some(index) = row_tail {
                            let tail = self.materialize(store, pools, semantic, TypeRef::Rigid(*index), substitutions)?;
                            let (names, types) = pools.record_fields(tail)?;
                            self.charge(names.len())?;
                            for (&name, &raw) in names.iter().zip(types) {
                                let ty = GroundTypeId::from_raw(raw).ok_or_else(|| failure("row field type id is invalid"))?;
                                if row.insert(name, ty).is_some() { return Err(failure("row substitution violates an explicit field lacks constraint")); }
                            }
                        }
                        build(semantic.intern_record_type(pools, &row.into_iter().collect::<Vec<_>>()))?
                    }
                    TypeTemplate::Arrow { kind, parameters, result, effects } => {
                        if parameters.len() > 65536 { return Err(failure("callable parameter count exceeds its bound")); }
                        self.charge(parameters.len().saturating_add(effects.len()))?;
                        let mut params = Vec::with_capacity(parameters.len());
                        for parameter in parameters {
                            let ty = self.materialize(store, pools, semantic, parameter.ty, substitutions)?;
                            let flags = u32::from(parameter.defaulted) | u32::from(parameter.rest) << 1 | u32::from(parameter.mode == ParameterMode::NamedOnly) << 2;
                            params.push((parameter.label, ty, flags));
                        }
                        let result = self.materialize(store, pools, semantic, *result, substitutions)?;
                        let signature = semantic.intern_signature_parts(pools, &params, result, Some(effects)).map_err(|_| failure("instantiated callable signature cannot enter the ground pool"))?;
                        if *kind == CallableKind::Pure && pools.signature_closed_effects(signature)? != crate::sema::inference::EffectSet::EMPTY { return Err(failure("pure callable template has effects")); }
                        build(semantic.intern_callable_descriptor(pools, *kind, signature))?
                    }
                };
                self.active.pop();
                ty
            }
        };
        self.memo.insert(reference, ty);
        Ok(ty)
    }
}

impl GenericEvidenceStore {
    pub(super) fn materialize_type(&self, reference: TypeRef, substitutions: &[GroundTypeId], pools: &mut SemanticPools, semantic: &mut SemanticPoolBuilder) -> Result<GroundTypeId, IrVerifyError> {
        let checkpoint = semantic.checkpoint(pools);
        let mut state = Materialization { memo: FxHashMap::default(), active: Vec::new(), work: 0 };
        let result = state.materialize(self, pools, semantic, reference, substitutions);
        // A rejected parent cannot leave its partially interned children usable
        // as a certificate for a later materialization.
        if result.is_err() { semantic.rewind(pools, checkpoint); }
        result
    }

    pub(super) fn normalized_ground_type(pools: &SemanticPools, ty: GroundTypeId) -> Result<NormalizedType, IrVerifyError> {
        fn read(pools: &SemanticPools, ty: GroundTypeId, active: &mut Vec<GroundTypeId>, work: &mut usize) -> Result<NormalizedType, IrVerifyError> {
            *work = work.checked_add(1).filter(|&work| work <= 2_000_000).ok_or_else(|| failure("ground type normalization exceeds its work bound"))?;
            if active.len() >= 256 || active.contains(&ty) { return Err(failure("ground type normalization is cyclic or exceeds its depth bound")); }
            active.push(ty);
            let result = match pools.type_tag(ty)? {
                TypeTag::Optional | TypeTag::List | TypeTag::Stream => {
                    let (child, _) = pools.type_children(ty)?.ok_or_else(|| failure("unary type child is missing"))?;
                    let child = Box::new(read(pools, child, active, work)?);
                    match pools.type_tag(ty)? { TypeTag::Optional => NormalizedType::Optional(child), TypeTag::List => NormalizedType::List(child), _ => NormalizedType::Stream(child) }
                }
                TypeTag::Map | TypeTag::Result => {
                    let (left, right) = pools.type_children(ty)?.ok_or_else(|| failure("pair type children are missing"))?;
                    let left = Box::new(read(pools, left, active, work)?);
                    let right = Box::new(read(pools, right.ok_or_else(|| failure("pair type child is missing"))?, active, work)?);
                    if pools.type_tag(ty)? == TypeTag::Map { NormalizedType::Map(left, right) } else { NormalizedType::Result(left, right) }
                }
                TypeTag::Record => {
                    let (names, types) = pools.record_fields(ty)?;
                    let mut fields = std::collections::BTreeMap::new();
                    for (&name, &raw) in names.iter().zip(types) {
                        let ty = GroundTypeId::from_raw(raw).ok_or_else(|| failure("record field type id is invalid"))?;
                        if fields.insert(name, read(pools, ty, active, work)?).is_some() { return Err(failure("record fields are not unique")); }
                    }
                    NormalizedType::Record(fields, None)
                }
                TypeTag::Callable => {
                    let (kind, signature) = pools.callable_descriptor(ty)?.ok_or_else(|| failure("callable descriptor is missing"))?;
                    let count = pools.signature_param_count(signature)?;
                    if count > 65536 { return Err(failure("callable parameter count exceeds its bound")); }
                    let mut parameters = Vec::with_capacity(count);
                    for index in 0..count {
                        let (label, ty, _) = pools.signature_param(signature, index)?;
                        parameters.push((label, pools.signature_parameter_mode(signature, index)?, pools.signature_parameter_defaulted(signature, index)?, pools.signature_parameter_rest(signature, index)?, read(pools, ty, active, work)?));
                    }
                    let effects = pools.signature_closed_effects(signature)?;
                    if kind == CallableKind::Pure && effects != crate::sema::inference::EffectSet::EMPTY { return Err(failure("pure callable descriptor has effects")); }
                    let result = read(pools, pools.signature_return_type(signature)?, active, work)?;
                    NormalizedType::Arrow(kind, parameters, Box::new(result), effect_names(effects))
                }
                _ => normalize_ground(pools.to_type(ty)?),
            };
            active.pop();
            Ok(result)
        }
        read(pools, ty, &mut Vec::new(), &mut 0)
    }

    pub(super) fn instantiated_normalized_type(&self, pools: &SemanticPools, scope: &SchemeScope, reference: TypeRef, substitutions: &[GroundTypeId]) -> Result<NormalizedType, IrVerifyError> {
        let substitutions = substitutions.iter().map(|&ty| Self::normalized_ground_type(pools, ty)).collect::<Result<Vec<_>, _>>()?;
        self.normalized(pools, scope, reference, Some(&substitutions), &mut Vec::new())
    }
}

pub(super) fn effect_names(effects: crate::sema::inference::EffectSet) -> Box<[crate::syntax::node::Effect]> {
    use crate::sema::inference::EffectSet;
    use crate::syntax::node::Effect;
    [(EffectSet::FS, Effect::Fs), (EffectSet::NET, Effect::Net), (EffectSet::PROCESS, Effect::Process), (EffectSet::ENV, Effect::Env), (EffectSet::TIME, Effect::Time), (EffectSet::ERROR, Effect::Error), (EffectSet::IO, Effect::Io)]
        .into_iter().filter_map(|(bit, effect)| (effects.0 & bit.0 != 0).then_some(effect)).collect::<Vec<_>>().into_boxed_slice()
}

pub(super) fn canonical_effect_names(effects: &[crate::syntax::node::Effect]) -> Box<[crate::syntax::node::Effect]> {
    use crate::sema::inference::EffectSet;
    use crate::syntax::node::Effect;
    let mut set = EffectSet::EMPTY;
    for effect in effects {
        set.0 |= match effect { Effect::Fs => EffectSet::FS.0, Effect::Net => EffectSet::NET.0, Effect::Process => EffectSet::PROCESS.0, Effect::Env => EffectSet::ENV.0, Effect::Time => EffectSet::TIME.0, Effect::Error => EffectSet::ERROR.0, Effect::Io => EffectSet::IO.0 };
    }
    effect_names(set)
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::runtime::eval::indexed::semantic::SemanticPoolBuilder;
    use crate::sema::types::Type;
    use crate::syntax::node::Effect;

    #[test]
    fn generic_callable_materialization_preserves_the_complete_signature() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut pools = SemanticPools::default();
        let mut semantic = SemanticPoolBuilder::default();
        let mut generic = GenericEvidenceBuilder::default();
        let rest = generic.add_template(TypeTemplate::List(TypeRef::Rigid(0))).unwrap();
        let arrow = generic.add_template(TypeTemplate::Arrow {
            kind: CallableKind::Proc,
            parameters: Box::new([
                TemplateParameter { label: Name::intern("operand"), ty: TypeRef::Rigid(0), mode: ParameterMode::PositionalOrNamed, defaulted: false, rest: false },
                TemplateParameter { label: Name::intern("fallback"), ty: TypeRef::Rigid(0), mode: ParameterMode::NamedOnly, defaulted: true, rest: false },
                TemplateParameter { label: Name::intern("remaining"), ty: TypeRef::Template(rest), mode: ParameterMode::PositionalOrNamed, defaulted: false, rest: true },
            ]),
            result: TypeRef::Rigid(0), effects: Box::new([Effect::Time]),
        }).unwrap();
        let nested = generic.add_template(TypeTemplate::List(TypeRef::Template(arrow))).unwrap();
        for actual in [Type::Int, Type::Str] {
            let actual = semantic.intern_type(&mut pools, &actual).unwrap();
            let callable = generic.materialize_reference(TypeRef::Template(arrow), &[actual], &mut pools, &mut semantic).expect("a closed Arrow template must retain a typed descriptor");
            let (kind, signature) = pools.callable_descriptor(callable).unwrap().unwrap();
            assert_eq!(kind, CallableKind::Proc);
            assert_eq!(pools.signature_return_type(signature).unwrap(), actual);
            assert_eq!(pools.signature_closed_effects(signature).unwrap(), crate::sema::inference::EffectSet::TIME);
            assert_eq!(pools.signature_param_count(signature).unwrap(), 3);
            assert_eq!(pools.signature_param(signature, 0).unwrap(), (Name::intern("operand"), actual, 0));
            assert_eq!(pools.signature_param(signature, 1).unwrap(), (Name::intern("fallback"), actual, 5));
            assert_eq!(pools.signature_parameter_mode(signature, 1).unwrap(), ParameterMode::NamedOnly);
            assert!(pools.signature_parameter_defaulted(signature, 1).unwrap());
            assert!(!pools.signature_parameter_rest(signature, 1).unwrap());
            let (_, rest_type, flags) = pools.signature_param(signature, 2).unwrap();
            assert_eq!(flags, 2);
            assert_eq!(pools.to_type(rest_type).unwrap(), Type::List(Box::new(pools.to_type(actual).unwrap())));
            let nested = generic.materialize_reference(TypeRef::Template(nested), &[actual], &mut pools, &mut semantic).expect("nested callable descriptors must not pass through legacy erasure");
            assert_eq!(pools.type_tag(nested).unwrap(), TypeTag::List);
            assert_eq!(pools.type_children(nested).unwrap(), Some((callable, None)));
            assert!(pools.to_type(callable).is_err());
            assert!(pools.to_type(nested).is_err());
            pools.verify().unwrap();
        }
    }

    #[test]
    fn callable_signature_named_only_flag_is_distinct_from_default_and_rest() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut pools = SemanticPools::default();
        let mut semantic = SemanticPoolBuilder::default();
        let value = semantic.intern_type(&mut pools, &Type::Int).unwrap();
        let signature = semantic.intern_signature_parts(&mut pools, &[(Name::intern("value"), value, 5)], value, Some(&[])).unwrap();
        semantic.intern_callable_descriptor(&mut pools, CallableKind::Pure, signature).unwrap();
        assert!(pools.verify().is_ok(), "named-only and default metadata must survive descriptor verification");
    }

    #[test]
    fn generic_callable_instance_proof_rejects_signature_metadata_substitution() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut pools = SemanticPools::default();
        let mut semantic = SemanticPoolBuilder::default();
        let mut generic = GenericEvidenceBuilder::default();
        let value = semantic.intern_type(&mut pools, &Type::Int).unwrap();
        let label = Name::intern("value");
        let template = generic.add_template(TypeTemplate::Arrow { kind: CallableKind::Pure, parameters: Box::new([TemplateParameter { label, ty: TypeRef::Rigid(0), mode: ParameterMode::NamedOnly, defaulted: true, rest: false }]), result: TypeRef::Rigid(0), effects: Box::new([]) }).unwrap();
        let callable = generic.materialize_reference(TypeRef::Template(template), &[value], &mut pools, &mut semantic).unwrap();
        let scope = generic.add_scope(SchemeScope { owner: super::super::super::IrFunctionId::new(0).unwrap(), quantifiers: Box::new([QuantifierKind::Type]), parameters: Box::new([TypeRef::Template(template)]), parameter_names: Box::new([Name::intern("callback")]), parameter_flags: Box::new([0]), kind: CallableKind::Pure, result: TypeRef::Rigid(0), requirements: Box::new([]), return_plan: GenericReturnPlan::Value }).unwrap();
        let instance = generic.add_instance(Instantiation { scope, substitutions: Box::new([value]), parameter_types: Box::new([callable]), result_type: value, requirements: Box::new([]) }).unwrap();
        let store = generic.finish(&pools, 1, &[]).expect("typed callable parameter must certify a concrete instance");
        for (label, flags) in [(Name::intern("other"), 5), (label, 1), (label, 4), (label, 7)] {
            let signature = semantic.intern_signature_parts(&mut pools, &[(label, value, flags)], value, Some(&[])).unwrap();
            let substituted = semantic.intern_callable_descriptor(&mut pools, CallableKind::Pure, signature).unwrap();
            let mut corrupted = store.clone();
            corrupted.instances[instance.index as usize].value.parameter_types[0] = substituted;
            assert!(corrupted.verify(&pools, 1, &[]).is_err(), "a full signature promise cannot be replaced by a same-result descriptor");
        }
        let text = semantic.intern_type(&mut pools, &Type::Str).unwrap();
        for (kind, parameter, result, effects) in [
            (CallableKind::Pure, text, value, Vec::new()),
            (CallableKind::Pure, value, text, Vec::new()),
            (CallableKind::Proc, value, value, Vec::new()),
            (CallableKind::Proc, value, value, vec![Effect::Time]),
        ] {
            let signature = semantic.intern_signature_parts(&mut pools, &[(label, parameter, 5)], result, Some(&effects)).unwrap();
            let substituted = semantic.intern_callable_descriptor(&mut pools, kind, signature).unwrap();
            let mut corrupted = store.clone();
            corrupted.instances[instance.index as usize].value.parameter_types[0] = substituted;
            assert!(corrupted.verify(&pools, 1, &[]).is_err(), "parameter, result, kind and closed effects are part of one signature promise");
        }
    }

    #[test]
    fn generic_callable_template_comes_from_the_original_scheme_signature() {
        use crate::sema::inference::{Arrow, EffectSet, EffectSummary, Generalization, InferenceContext, Parameter};
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut graph = InferenceContext::default();
        let span = crate::source::Span::new(crate::source::SourceId::new(0), 0, 1);
        let value = graph.fresh(1, span).unwrap();
        let root = graph.arrow(Arrow { kind: crate::sema::inference::CallableKind::Pure, params: vec![Parameter { label: Name::intern("actual_name"), ty: value, defaulted: true, rest: false }], result: value, effects: EffectSummary::Closed(EffectSet::EMPTY) }).unwrap();
        let scheme = graph.generalize(root, 0, Generalization::Allowed, &[]).unwrap();
        let mut generic = GenericEvidenceBuilder::default();
        let mut pools = SemanticPools::default();
        let mut semantic = SemanticPoolBuilder::default();
        let reference = generic.prepare_reference(&graph, scheme, root, &mut pools, &mut semantic).expect("original scoped Arrow must retain its definition-owned relation");
        let TypeRef::Template(template) = reference else { panic!("generic Arrow requires a scoped template"); };
        let TypeTemplate::Arrow { kind, parameters, result, effects } = generic.template(template).unwrap() else { panic!("original Arrow must remain an Arrow"); };
        assert_eq!(*kind, CallableKind::Pure);
        assert_eq!(parameters[0].label, Name::intern("actual_name"));
        assert_eq!(parameters[0].ty, TypeRef::Rigid(0));
        assert_eq!(*result, TypeRef::Rigid(0));
        assert!(parameters[0].defaulted);
        assert!(effects.is_empty());
    }

    #[test]
    fn generic_callable_materialization_rewinds_failed_children_and_bounds_depth() {
        let symbols = crate::symbol::SymbolOwner::new();
        let _symbols = symbols.enter();
        let mut pools = SemanticPools::default();
        let mut semantic = SemanticPoolBuilder::default();
        let mut generic = GenericEvidenceBuilder::default();
        let value = semantic.intern_type(&mut pools, &Type::Int).unwrap();
        let list = generic.add_template(TypeTemplate::List(TypeRef::Rigid(0))).unwrap();
        let bad = generic.add_template(TypeTemplate::Arrow { kind: CallableKind::Pure, parameters: Box::new([TemplateParameter { label: Name::intern("value"), ty: TypeRef::Template(list), mode: ParameterMode::PositionalOrNamed, defaulted: false, rest: false }]), result: TypeRef::Rigid(0), effects: Box::new([Effect::Time]) }).unwrap();
        assert!(generic.materialize_reference(TypeRef::Template(bad), &[value], &mut pools, &mut semantic).is_err());
        assert_eq!(semantic.intern_type(&mut pools, &Type::Int).unwrap(), value);
        assert_eq!(pools.type_tag(GroundTypeId::new(value.index() + 1).unwrap()).is_err(), true);
        pools.verify().unwrap();
        let mut deep = TypeRef::Rigid(0);
        for _ in 0..257 { deep = TypeRef::Template(generic.add_template(TypeTemplate::List(deep)).unwrap()); }
        assert!(generic.materialize_reference(deep, &[value], &mut pools, &mut semantic).is_err());
        assert!(pools.type_tag(GroundTypeId::new(value.index() + 1).unwrap()).is_err());
    }
}
