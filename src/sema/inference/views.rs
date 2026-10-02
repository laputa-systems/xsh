use super::*;
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
use crate::syntax::node::Effect;
use std::collections::BTreeMap;

impl Atom {
    pub(crate) fn from_type(ty: &Type) -> Option<Self> {
        Some(match ty {
            Type::Any => Atom::Any, Type::ErasedRecord => Atom::ErasedRecord, Type::DynamicModule => Atom::DynamicModule,
            Type::Null => Atom::Null, Type::Bool => Atom::Bool, Type::Int => Atom::Int, Type::UInt => Atom::UInt,
            Type::Float => Atom::Float, Type::Duration => Atom::Duration, Type::Str => Atom::Str, Type::Bytes => Atom::Bytes,
            Type::Digest => Atom::Digest, Type::Regex => Atom::Regex, Type::Path => Atom::Path, Type::Unit => Atom::Unit,
            Type::Status => Atom::Status, Type::EnvPathList => Atom::EnvPathList, Type::Error => Atom::Error,
            Type::ProcessError => Atom::ProcessError, Type::ProcessHandle => Atom::ProcessHandle, Type::NetJob => Atom::NetJob, Type::FsRoot => Atom::FsRoot,
            Type::Pure => Atom::Pure, Type::Proc => Atom::Proc, Type::Command => Atom::Command,
            Type::Tag(name) => Atom::Tag(*name), Type::ErrorFamily(name) => Atom::ErrorFamily(*name), Type::ErrorFacet(name) => Atom::ErrorFacet(*name),
            Type::ErrorVariant { family, variant } => Atom::ErrorVariant { family: *family, variant: *variant },
            _ => return None,
        })
    }
}

impl InferenceContext {
    /// Trees are bounded input/output views of the graph. They never decide
    /// compatibility, infer generic relationships, or manufacture dynamic types.
    pub fn import_type(&mut self, ty: &Type, level: u32, origin: Span) -> Result<TypeId, InferenceError> {
        self.probe(|graph| graph.import_view(ty, level, origin, 0, false))
    }
    /// Legacy identities are keys into this graph, never independent answers.
    /// Constraints must target the returned handle; no legacy substitution is
    /// consulted and dynamic or recovery views do not become metavariables.
    pub fn shared_variable(&mut self, id: TypeVariableId, level: u32, origin: Span) -> Result<TypeId, InferenceError> {
        self.probe(|graph| {
            graph.work()?;
            if let Some(previous) = graph.legacy_variables.get(&id).copied() {
                graph.node(previous.ty)?;
                if previous.level > level {
                    let why = graph.reason(origin, None)?; graph.capture(previous.ty, level, why)?;
                    graph.trail.push(Trail::LegacyVariable(id, Some(previous)));
                    graph.legacy_variables.insert(id, LegacyVariable { ty: previous.ty, level });
                }
                return Ok(previous.ty);
            }
            let ty = graph.fresh(level, origin)?;
            graph.trail.push(Trail::LegacyVariable(id, None));
            graph.legacy_variables.insert(id, LegacyVariable { ty, level });
            Ok(ty)
        })
    }
    pub fn import_type_shared(&mut self, ty: &Type, level: u32, origin: Span) -> Result<TypeId, InferenceError> {
        self.probe(|graph| graph.import_view(ty, level, origin, 0, true))
    }
    fn import_view(&mut self, ty: &Type, level: u32, origin: Span, depth: usize, shared: bool) -> Result<TypeId, InferenceError> {
        self.work()?;
        if depth > self.limits.structural_depth { return Err(InferenceError::Limit("boundary view depth")); }
        let atom = match ty {
            Type::Graph(id) => { self.node(*id)?; return Ok(*id); }
            Type::Unknown | Type::Invalid => return self.poison(),
            Type::Inference(id) if shared => return self.shared_variable(*id, level, origin),
            Type::Inference(_) => return Err(InferenceError::Boundary("legacy variable must be resolved before graph import")),
            Type::BuiltinParameter(_) => return Err(InferenceError::Boundary("builtin parameter must be instantiated before graph import")),
            Type::List(item) | Type::Optional(item) | Type::Stream(item) => {
                let item = self.import_view(item, level, origin, depth + 1, shared)?;
                return match ty { Type::List(_) => self.list(item), Type::Optional(_) => self.optional(item), _ => self.stream(item) };
            }
            Type::Map(a, b) | Type::Result(a, b) => {
                let a = self.import_view(a, level, origin, depth + 1, shared)?; let b = self.import_view(b, level, origin, depth + 1, shared)?;
                return match ty { Type::Map(_, _) => self.map(a, b), _ => self.result(a, b) };
            }
            Type::Record(fields) => {
                let mut row = Vec::with_capacity(fields.len());
                for (label, ty) in fields { row.push(RowField { label: *label, ty: self.import_view(ty, level, origin, depth + 1, shared)? }); }
                let row = self.row(row, None)?; return self.record(row);
            }
            Type::Module(exports) => {
                let mut fields = Vec::with_capacity(exports.len());
                for (label, export) in exports {
                    let ty = match export {
                        ModuleExportType::Value { ty, .. } => self.import_view(ty, level, origin, depth + 1, shared)?,
                        ModuleExportType::Pure { sig, .. } => self.import_callable(sig, CallableKind::Pure, level, origin, depth + 1, shared)?,
                        ModuleExportType::Proc { sig, .. } => self.import_callable(sig, CallableKind::Proc, level, origin, depth + 1, shared)?,
                    };
                    fields.push(ModuleField { label: *label, ty, optional: export.optional() });
                }
                return self.module(fields);
            }
            ty => Atom::from_type(ty).ok_or(InferenceError::Boundary("type view is not an atom"))?,
        };
        self.atom(atom)
    }
    fn import_callable(&mut self, signature: &CallableType, kind: CallableKind, level: u32, origin: Span, depth: usize, shared: bool) -> Result<TypeId, InferenceError> {
        let mut params = Vec::with_capacity(signature.params.len());
        for parameter in &signature.params { params.push(Parameter { label: parameter.name, ty: self.import_view(&parameter.ty, level, origin, depth + 1, shared)?, defaulted: parameter.defaulted, rest: parameter.rest }); }
        let result = self.import_view(&signature.return_ty, level, origin, depth + 1, shared)?;
        let effects = if kind == CallableKind::Pure { EffectSummary::Closed(EffectSet::EMPTY) }
            else { signature.effects.as_ref().map(|effects| EffectSummary::Closed(effect_bits(effects))).unwrap_or(EffectSummary::Unknown) };
        self.arrow(Arrow { kind, params, result, effects })
    }
    pub fn export_type(&self, ty: TypeId) -> Result<Type, InferenceError> { self.export_view(ty, 0) }
    fn export_view(&self, ty: TypeId, depth: usize) -> Result<Type, InferenceError> {
        if depth > self.limits.structural_depth { return Err(InferenceError::Limit("boundary view depth")); }
        let ty = self.resolved(ty)?;
        Ok(match self.node(ty)? {
            TypeNode::Atom(atom) => match atom {
                Atom::Any => Type::Any, Atom::ErasedRecord => Type::ErasedRecord, Atom::DynamicModule => Type::DynamicModule,
                Atom::Pure => Type::Pure, Atom::Proc => Type::Proc, Atom::Command => Type::Command,
                Atom::Null => Type::Null, Atom::Bool => Type::Bool, Atom::Int => Type::Int, Atom::UInt => Type::UInt, Atom::Float => Type::Float,
                Atom::Duration => Type::Duration, Atom::Str => Type::Str, Atom::Bytes => Type::Bytes, Atom::Digest => Type::Digest,
                Atom::Regex => Type::Regex, Atom::Path => Type::Path, Atom::Unit => Type::Unit, Atom::Status => Type::Status,
                Atom::EnvPathList => Type::EnvPathList, Atom::Error => Type::Error, Atom::ProcessError => Type::ProcessError,
                Atom::ProcessHandle => Type::ProcessHandle, Atom::NetJob => Type::NetJob, Atom::FsRoot => Type::FsRoot,
                Atom::Tag(name) => Type::Tag(*name), Atom::ErrorFamily(name) => Type::ErrorFamily(*name), Atom::ErrorFacet(name) => Type::ErrorFacet(*name),
                Atom::ErrorVariant { family, variant } => Type::ErrorVariant { family: *family, variant: *variant },
            },
            TypeNode::List(item) => Type::List(Box::new(self.export_view(*item, depth + 1)?)),
            TypeNode::Optional(item) => {let item=self.export_view(*item,depth+1)?;if matches!(item,Type::Optional(_)){item}else{Type::Optional(Box::new(item))}},
            TypeNode::Stream(item) => Type::Stream(Box::new(self.export_view(*item, depth + 1)?)),
            TypeNode::Map(key, value) => Type::Map(Box::new(self.export_view(*key, depth + 1)?), Box::new(self.export_view(*value, depth + 1)?)),
            TypeNode::Result(ok, error) => Type::Result(Box::new(self.export_view(*ok, depth + 1)?), Box::new(self.export_view(*error, depth + 1)?)),
            TypeNode::Record(row) => Type::Record(self.export_row(*row, depth + 1)?),
            TypeNode::Module(exports) => {
                let mut fields = BTreeMap::new();
                for field in exports {
                    let export = match self.node(self.resolved(field.ty)?)? {
                        TypeNode::Arrow(arrow) => {
                            let sig = self.export_callable(arrow, depth + 1)?;
                            match arrow.kind { CallableKind::Pure => ModuleExportType::Pure { sig, optional: field.optional }, CallableKind::Proc => ModuleExportType::Proc { sig, optional: field.optional }, CallableKind::Stream => return Err(InferenceError::Boundary("stream export view")) }
                        }
                        _ => ModuleExportType::Value { ty: self.export_view(field.ty, depth + 1)?, optional: field.optional },
                    };
                    fields.insert(field.label, export);
                }
                Type::Module(fields)
            }
            TypeNode::Meta(_) | TypeNode::Rigid { .. } | TypeNode::Row(_) | TypeNode::Arrow(_) | TypeNode::NativeCallable(_) | TypeNode::CallableChoice(_) | TypeNode::FiniteDomain(_) => return Err(InferenceError::Unresolved(ty)),
            TypeNode::Poison | TypeNode::NonCompletion => return Err(InferenceError::Recovery(ty)),
        })
    }
    fn export_row(&self, row: RowId, depth: usize) -> Result<BTreeMap<Name, Type>, InferenceError> {
        if depth > self.limits.structural_depth { return Err(InferenceError::Limit("boundary view depth")); }
        let row = self.row_data(row)?; let mut fields = BTreeMap::new();
        for field in &row.fields { fields.insert(field.label, self.export_view(field.ty, depth + 1)?); }
        if let Some(tail) = row.tail {
            let tail = self.resolved(tail)?;
            let TypeNode::Row(row) = self.node(tail)? else { return Err(InferenceError::Unresolved(tail)) };
            for (label, ty) in self.export_row(*row, depth + 1)? { if fields.insert(label, ty).is_some() { return Err(InferenceError::DuplicateLabel(label)); } }
        }
        Ok(fields)
    }
    fn export_callable(&self, arrow: &Arrow, depth: usize) -> Result<CallableType, InferenceError> {
        let mut params = Vec::with_capacity(arrow.params.len());
        for parameter in &arrow.params { params.push(CallableParamType { name: parameter.label, ty: self.export_view(parameter.ty, depth + 1)?, defaulted: parameter.defaulted, rest: parameter.rest }); }
        let effects = match self.resolved_effect_summary(arrow.effects)? { EffectSummary::Closed(bits) => Some(effect_list(bits)), EffectSummary::Variable(id) => Some(effect_list(self.effect_value(id)?)), EffectSummary::Rigid { .. } => return Err(InferenceError::Boundary("generic latent effects have no ground view")), EffectSummary::Unknown => None };
        Ok(CallableType { params, return_ty: Box::new(self.export_view(arrow.result, depth + 1)?), effects })
    }
}

fn effect_bits(effects: &[Effect]) -> EffectSet {
    let mut bits = 0;
    for effect in effects { bits |= match effect { Effect::Fs => EffectSet::FS.0, Effect::Net => EffectSet::NET.0, Effect::Process => EffectSet::PROCESS.0, Effect::Env => EffectSet::ENV.0, Effect::Time => EffectSet::TIME.0, Effect::Error => EffectSet::ERROR.0, Effect::Io => EffectSet::IO.0 }; }
    EffectSet(bits)
}

fn effect_list(bits: EffectSet) -> Vec<Effect> {
    [(EffectSet::FS, Effect::Fs), (EffectSet::NET, Effect::Net), (EffectSet::PROCESS, Effect::Process), (EffectSet::ENV, Effect::Env), (EffectSet::TIME, Effect::Time), (EffectSet::ERROR, Effect::Error), (EffectSet::IO, Effect::Io)].into_iter().filter_map(|(bit, effect)| if bits.0 & bit.0 != 0 { Some(effect) } else { None }).collect()
}
