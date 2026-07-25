use super::{
    IR_NONE, IrBuildError, IrData, IrRange, IrVerifyError, ShapeId, SignatureId, TypeId,
};
use crate::sema::types::{CallableType, ModuleExportType, Type};
use crate::symbol::{Name, Symbol};
use crate::syntax::node::Effect;
use rustc_hash::FxHashMap;
use std::mem::size_of;

const PARAM_DEFAULTED: u32 = 1;
const PARAM_REST: u32 = 1 << 1;
const MODULE_EXPORT_OPTIONAL: u32 = 1 << 2;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub(super) enum TypeTag {
    Any,
    Null,
    Bool,
    Int,
    Float,
    Duration,
    Str,
    Bytes,
    Digest,
    Regex,
    Path,
    List,
    Map,
    Stream,
    Record,
    Module,
    Result,
    Status,
    EnvPathList,
    Error,
    ErrorFamily,
    ErrorVariant,
    ErrorFacet,
    ProcessError,
    Pure,
    Proc,
    Command,
    ProcessHandle,
    Unit,
    Tag,
    Optional,
}

impl TypeTag {
    fn has_no_payload(self) -> bool {
        matches!(
            self,
            Self::Any
                | Self::Null
                | Self::Bool
                | Self::Int
                | Self::Float
                | Self::Duration
                | Self::Str
                | Self::Bytes
                | Self::Digest
                | Self::Regex
                | Self::Path
                | Self::Status
                | Self::EnvPathList
                | Self::Error
                | Self::ProcessError
                | Self::Pure
                | Self::Proc
                | Self::Command
                | Self::ProcessHandle
                | Self::Unit
        )
    }

    fn has_one_type(self) -> bool {
        matches!(self, Self::List | Self::Map | Self::Stream | Self::Optional)
    }

    fn has_one_name(self) -> bool {
        matches!(
            self,
            Self::ErrorFamily | Self::ErrorFacet | Self::Tag
        )
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct SemanticPools {
    type_tags: Vec<TypeTag>,
    type_data: Vec<IrData>,
    type_extra: Vec<u32>,
    signature_data: Vec<IrData>,
    signature_extra: Vec<u32>,
    shapes: Vec<IrRange>,
    shape_fields: Vec<Name>,
}

impl SemanticPools {
    pub(super) fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            + self.type_tags.capacity() * size_of::<TypeTag>()
            + self.type_data.capacity() * size_of::<IrData>()
            + self.type_extra.capacity() * size_of::<u32>()
            + self.signature_data.capacity() * size_of::<IrData>()
            + self.signature_extra.capacity() * size_of::<u32>()
            + self.shapes.capacity() * size_of::<IrRange>()
            + self.shape_fields.capacity() * size_of::<Name>()
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.type_tags.shrink_to_fit();
        self.type_data.shrink_to_fit();
        self.type_extra.shrink_to_fit();
        self.signature_data.shrink_to_fit();
        self.signature_extra.shrink_to_fit();
        self.shapes.shrink_to_fit();
        self.shape_fields.shrink_to_fit();
    }

    pub(super) fn type_count(&self) -> usize {
        self.type_tags.len()
    }

    pub(super) fn signature_count(&self) -> usize {
        self.signature_data.len()
    }

    pub(super) fn shape_count(&self) -> usize {
        self.shapes.len()
    }

    pub(super) fn extra_words(&self) -> usize {
        self.type_extra.len() + self.signature_extra.len()
    }

    pub(super) fn type_tag(&self, id: TypeId) -> Result<TypeTag, IrVerifyError> {
        self.type_tags
            .get(id.index())
            .copied()
            .ok_or_else(|| IrVerifyError::new("type id is out of bounds"))
    }

    pub(super) fn signature_return_type(
        &self,
        id: SignatureId,
    ) -> Result<TypeId, IrVerifyError> {
        let payload = self.signature_payload(id)?;
        TypeId::from_raw(payload[0])
            .ok_or_else(|| IrVerifyError::new("signature return type id is invalid"))
    }

    pub(super) fn signature_param_count(
        &self,
        id: SignatureId,
    ) -> Result<usize, IrVerifyError> {
        Ok(self.signature_payload(id)?[2] as usize)
    }

    pub(super) fn signature_param(
        &self,
        id: SignatureId,
        index: usize,
    ) -> Result<(Name, TypeId, u32), IrVerifyError> {
        let payload = self.signature_payload(id)?;
        let effects = signature_effect_count(payload)?;
        let params = payload[2] as usize;
        if index >= params {
            return Err(IrVerifyError::new(
                "signature parameter index is out of bounds",
            ));
        }
        let start = 3 + effects + index * 3;
        let ty = TypeId::from_raw(payload[start + 1])
            .ok_or_else(|| IrVerifyError::new("signature parameter type id is invalid"))?;
        Ok((
            Name::from_symbol(Symbol::from_raw(payload[start])),
            ty,
            payload[start + 2],
        ))
    }

    pub(super) fn record_fields(
        &self,
        id: TypeId,
    ) -> Result<(&[Name], &[u32]), IrVerifyError> {
        if self.type_tag(id)? != TypeTag::Record {
            return Err(IrVerifyError::new("type id does not denote a record"));
        }
        let data = self.type_data[id.index()];
        let shape = ShapeId::from_raw(data.lhs)
            .ok_or_else(|| IrVerifyError::new("record shape id is invalid"))?;
        let fields = self.shape_fields(shape)?;
        let start = data.rhs as usize;
        let end = start
            .checked_add(fields.len())
            .ok_or_else(|| IrVerifyError::new("record type payload overflows"))?;
        let types = self
            .type_extra
            .get(start..end)
            .ok_or_else(|| IrVerifyError::new("record type payload is out of bounds"))?;
        Ok((fields, types))
    }

    pub(super) fn display_type(&self, id: TypeId) -> Result<String, IrVerifyError> {
        self.display_type_inner(id, 0)
    }

    fn display_type_inner(
        &self,
        id: TypeId,
        depth: usize,
    ) -> Result<String, IrVerifyError> {
        if depth > self.type_tags.len() {
            return Err(IrVerifyError::new("type graph contains a cycle"));
        }
        let tag = self.type_tag(id)?;
        let data = self.type_data[id.index()];
        let scalar = match tag {
            TypeTag::Any => Some("Any"),
            TypeTag::Null => Some("Null"),
            TypeTag::Bool => Some("Bool"),
            TypeTag::Int => Some("Int"),
            TypeTag::Float => Some("Float"),
            TypeTag::Duration => Some("Duration"),
            TypeTag::Str => Some("Str"),
            TypeTag::Bytes => Some("Bytes"),
            TypeTag::Digest => Some("Digest"),
            TypeTag::Regex => Some("Regex"),
            TypeTag::Path => Some("Path"),
            TypeTag::Record => Some("Record"),
            TypeTag::Module => Some("Module"),
            TypeTag::Status => Some("Status"),
            TypeTag::EnvPathList => Some("EnvPathList"),
            TypeTag::Error => Some("Error"),
            TypeTag::ProcessError => Some("ProcessError"),
            TypeTag::Pure => Some("Pure"),
            TypeTag::Proc => Some("Proc"),
            TypeTag::Command => Some("Command"),
            TypeTag::ProcessHandle => Some("ProcessHandle"),
            TypeTag::Unit => Some("Unit"),
            _ => None,
        };
        if let Some(name) = scalar {
            return Ok(name.to_string());
        }
        if tag.has_one_type() {
            let inner = TypeId::from_raw(data.lhs)
                .ok_or_else(|| IrVerifyError::new("inner type id is invalid"))?;
            let inner = self.display_type_inner(inner, depth + 1)?;
            return Ok(match tag {
                TypeTag::List => format!("List[{inner}]"),
                TypeTag::Map => format!("Map[{inner}]"),
                TypeTag::Stream => format!("Stream[{inner}]"),
                TypeTag::Optional => format!("{inner}?"),
                _ => unreachable!("one-type tags are exhaustive"),
            });
        }
        if tag.has_one_name() {
            return Ok(Name::from_symbol(Symbol::from_raw(data.lhs)).to_string());
        }
        match tag {
            TypeTag::Result => {
                let ok = TypeId::from_raw(data.lhs)
                    .ok_or_else(|| IrVerifyError::new("result ok type id is invalid"))?;
                let err = TypeId::from_raw(data.rhs)
                    .ok_or_else(|| IrVerifyError::new("result error type id is invalid"))?;
                Ok(format!(
                    "Result[{}, {}]",
                    self.display_type_inner(ok, depth + 1)?,
                    self.display_type_inner(err, depth + 1)?
                ))
            }
            TypeTag::ErrorVariant => Ok(format!(
                "{}.{}",
                Name::from_symbol(Symbol::from_raw(data.lhs)),
                Name::from_symbol(Symbol::from_raw(data.rhs))
            )),
            _ => Err(IrVerifyError::new("type tag has no display schema")),
        }
    }

    fn signature_payload(&self, id: SignatureId) -> Result<&[u32], IrVerifyError> {
        let range = *self
            .signature_data
            .get(id.index())
            .ok_or_else(|| IrVerifyError::new("signature id is out of bounds"))?;
        let bounds = range
            .range()
            .bounds(self.signature_extra.len())
            .ok_or_else(|| IrVerifyError::new("signature payload is out of bounds"))?;
        Ok(&self.signature_extra[bounds])
    }

    fn shape_fields(&self, id: ShapeId) -> Result<&[Name], IrVerifyError> {
        let range = *self
            .shapes
            .get(id.index())
            .ok_or_else(|| IrVerifyError::new("shape id is out of bounds"))?;
        let bounds = range
            .bounds(self.shape_fields.len())
            .ok_or_else(|| IrVerifyError::new("shape field range is out of bounds"))?;
        Ok(&self.shape_fields[bounds])
    }

    pub(super) fn verify(&self) -> Result<(), IrVerifyError> {
        if self.type_tags.len() != self.type_data.len() {
            return Err(IrVerifyError::new(
                "type tag and data columns have different lengths",
            ));
        }
        for shape in &self.shapes {
            if shape.bounds(self.shape_fields.len()).is_none() {
                return Err(IrVerifyError::new("shape field range is out of bounds"));
            }
        }
        for index in 0..self.signature_data.len() {
            let id = SignatureId::new(index)
                .map_err(|_| IrVerifyError::new("signature id overflows"))?;
            let payload = self.signature_payload(id)?;
            if payload.len() < 3 {
                return Err(IrVerifyError::new("signature payload ended early"));
            }
            verify_type_raw(self, payload[0], None)?;
            let effects = signature_effect_count(payload)?;
            let params = payload[2] as usize;
            let expected = 3usize
                .checked_add(effects)
                .and_then(|len| len.checked_add(params.checked_mul(3)?))
                .ok_or_else(|| IrVerifyError::new("signature payload length overflows"))?;
            if payload.len() != expected {
                return Err(IrVerifyError::new(
                    "signature parameter count does not match payload",
                ));
            }
            if payload[1] != IR_NONE
                && payload[3..3 + effects]
                    .iter()
                    .any(|effect| *effect > EffectCode::Io as u32)
            {
                return Err(IrVerifyError::new("signature effect is invalid"));
            }
            for param in payload[3 + effects..].chunks_exact(3) {
                verify_type_raw(self, param[1], None)?;
                if param[2] & !(PARAM_DEFAULTED | PARAM_REST) != 0 {
                    return Err(IrVerifyError::new(
                        "signature parameter flags are invalid",
                    ));
                }
            }
        }
        for (index, (tag, data)) in self
            .type_tags
            .iter()
            .copied()
            .zip(self.type_data.iter().copied())
            .enumerate()
        {
            if tag.has_no_payload() {
                if data != IrData::ZERO {
                    return Err(IrVerifyError::new("scalar type has nonzero data"));
                }
                continue;
            }
            if tag.has_one_type() {
                verify_type_raw(self, data.lhs, Some(index))?;
                if data.rhs != 0 {
                    return Err(IrVerifyError::new("unary type has invalid data"));
                }
                continue;
            }
            if tag.has_one_name() {
                if data.rhs != 0 {
                    return Err(IrVerifyError::new("named type has invalid data"));
                }
                continue;
            }
            match tag {
                TypeTag::Result => {
                    verify_type_raw(self, data.lhs, Some(index))?;
                    verify_type_raw(self, data.rhs, Some(index))?;
                }
                TypeTag::ErrorVariant => {}
                TypeTag::Record => {
                    let id = TypeId::new(index)
                        .map_err(|_| IrVerifyError::new("type id overflows"))?;
                    let (_, fields) = self.record_fields(id)?;
                    for raw in fields {
                        verify_type_raw(self, *raw, Some(index))?;
                    }
                }
                TypeTag::Module => {
                    let shape = ShapeId::from_raw(data.lhs)
                        .ok_or_else(|| IrVerifyError::new("module shape id is invalid"))?;
                    let fields = self.shape_fields(shape)?;
                    let start = data.rhs as usize;
                    let len = fields
                        .len()
                        .checked_mul(2)
                        .ok_or_else(|| IrVerifyError::new("module payload length overflows"))?;
                    let end = start
                        .checked_add(len)
                        .ok_or_else(|| IrVerifyError::new("module payload range overflows"))?;
                    let exports =
                        self.type_extra.get(start..end).ok_or_else(|| {
                            IrVerifyError::new("module payload is out of bounds")
                        })?;
                    for export in exports.chunks_exact(2) {
                        let kind = export[0] & 0b11;
                        if export[0] & !(0b11 | MODULE_EXPORT_OPTIONAL) != 0 || kind > 2 {
                            return Err(IrVerifyError::new(
                                "module export flags are invalid",
                            ));
                        }
                        if kind == 0 {
                            verify_type_raw(self, export[1], Some(index))?;
                        } else {
                            let signature = SignatureId::from_raw(export[1]).ok_or_else(|| {
                                IrVerifyError::new("module export signature id is invalid")
                            })?;
                            if signature.index() >= self.signature_data.len() {
                                return Err(IrVerifyError::new(
                                    "module export signature id is out of bounds",
                                ));
                            }
                        }
                    }
                }
                _ => return Err(IrVerifyError::new("type tag has no verification schema")),
            }
        }
        Ok(())
    }
}

fn signature_effect_count(payload: &[u32]) -> Result<usize, IrVerifyError> {
    let raw = *payload
        .get(1)
        .ok_or_else(|| IrVerifyError::new("signature effect count is missing"))?;
    Ok(if raw == IR_NONE { 0 } else { raw as usize })
}

fn verify_type_raw(
    pools: &SemanticPools,
    raw: u32,
    before: Option<usize>,
) -> Result<TypeId, IrVerifyError> {
    let id =
        TypeId::from_raw(raw).ok_or_else(|| IrVerifyError::new("type id is invalid"))?;
    if id.index() >= pools.type_tags.len() {
        return Err(IrVerifyError::new("type id is out of bounds"));
    }
    if before.is_some_and(|before| id.index() >= before) {
        return Err(IrVerifyError::new(
            "type child does not precede its owning type",
        ));
    }
    Ok(id)
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
enum EffectCode {
    Fs,
    Net,
    Process,
    Env,
    Time,
    Error,
    Io,
}

impl From<&Effect> for EffectCode {
    fn from(effect: &Effect) -> Self {
        match effect {
            Effect::Fs => Self::Fs,
            Effect::Net => Self::Net,
            Effect::Process => Self::Process,
            Effect::Env => Self::Env,
            Effect::Time => Self::Time,
            Effect::Error => Self::Error,
            Effect::Io => Self::Io,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum TypeKey {
    Scalar(TypeTag),
    Unary(TypeTag, TypeId),
    Pair(TypeTag, TypeId, TypeId),
    Named(TypeTag, Name),
    NamedPair(TypeTag, Name, Name),
    Aggregate(TypeTag, ShapeId, Box<[u32]>),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct SignatureParamKey {
    name: Name,
    ty: TypeId,
    flags: u32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct SignatureKey {
    return_type: TypeId,
    effects: Option<Box<[EffectCode]>>,
    params: Box<[SignatureParamKey]>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct SemanticCheckpoint {
    types: usize,
    type_extra: usize,
    signatures: usize,
    signature_extra: usize,
    shapes: usize,
    shape_fields: usize,
}

#[derive(Default)]
pub(super) struct SemanticPoolBuilder {
    types: FxHashMap<TypeKey, TypeId>,
    signatures: FxHashMap<SignatureKey, SignatureId>,
    shapes: FxHashMap<Box<[Name]>, ShapeId>,
}

impl SemanticPoolBuilder {
    pub(super) fn retained_bytes(&self) -> usize {
        let type_key_bytes = self
            .types
            .keys()
            .map(|key| match key {
                TypeKey::Aggregate(_, _, words) => words.len() * size_of::<u32>(),
                _ => 0,
            })
            .sum::<usize>();
        let signature_key_bytes = self
            .signatures
            .keys()
            .map(|key| {
                key.effects
                    .as_ref()
                    .map_or(0, |effects| effects.len() * size_of::<EffectCode>())
                    + key.params.len() * size_of::<SignatureParamKey>()
            })
            .sum::<usize>();
        let shape_key_bytes = self
            .shapes
            .keys()
            .map(|fields| fields.len() * size_of::<Name>())
            .sum::<usize>();
        size_of::<Self>()
            + self.types.capacity() * size_of::<(TypeKey, TypeId)>()
            + type_key_bytes
            + self.signatures.capacity() * size_of::<(SignatureKey, SignatureId)>()
            + signature_key_bytes
            + self.shapes.capacity() * size_of::<(Box<[Name]>, ShapeId)>()
            + shape_key_bytes
    }

    pub(super) fn checkpoint(&self, pools: &SemanticPools) -> SemanticCheckpoint {
        SemanticCheckpoint {
            types: pools.type_tags.len(),
            type_extra: pools.type_extra.len(),
            signatures: pools.signature_data.len(),
            signature_extra: pools.signature_extra.len(),
            shapes: pools.shapes.len(),
            shape_fields: pools.shape_fields.len(),
        }
    }

    pub(super) fn rewind(
        &mut self,
        pools: &mut SemanticPools,
        checkpoint: SemanticCheckpoint,
    ) {
        pools.type_tags.truncate(checkpoint.types);
        pools.type_data.truncate(checkpoint.types);
        pools.type_extra.truncate(checkpoint.type_extra);
        pools.signature_data.truncate(checkpoint.signatures);
        pools.signature_extra.truncate(checkpoint.signature_extra);
        pools.shapes.truncate(checkpoint.shapes);
        pools.shape_fields.truncate(checkpoint.shape_fields);
        self.types.retain(|_, id| id.index() < checkpoint.types);
        self.signatures
            .retain(|_, id| id.index() < checkpoint.signatures);
        self.shapes
            .retain(|_, id| id.index() < checkpoint.shapes);
    }

    pub(super) fn intern_type(
        &mut self,
        pools: &mut SemanticPools,
        ty: &Type,
    ) -> Result<TypeId, IrBuildError> {
        let (key, data, extra) = match ty {
            Type::Unknown | Type::Invalid => {
                return Err(IrBuildError::format(
                    "recovery_type",
                    None,
                    0,
                    0,
                ));
            }
            Type::Any => scalar(TypeTag::Any),
            Type::Null => scalar(TypeTag::Null),
            Type::Bool => scalar(TypeTag::Bool),
            Type::Int => scalar(TypeTag::Int),
            Type::Float => scalar(TypeTag::Float),
            Type::Duration => scalar(TypeTag::Duration),
            Type::Str => scalar(TypeTag::Str),
            Type::Bytes => scalar(TypeTag::Bytes),
            Type::Digest => scalar(TypeTag::Digest),
            Type::Regex => scalar(TypeTag::Regex),
            Type::Path => scalar(TypeTag::Path),
            Type::List(inner) => self.unary(pools, TypeTag::List, inner)?,
            Type::Map(inner) => self.unary(pools, TypeTag::Map, inner)?,
            Type::Stream(inner) => self.unary(pools, TypeTag::Stream, inner)?,
            Type::Record(fields) => {
                let names = fields.keys().copied().collect::<Vec<_>>();
                let shape = self.intern_shape(pools, &names)?;
                let mut words = Vec::with_capacity(fields.len());
                for field in fields.values() {
                    words.push(self.intern_type(pools, field)?.raw());
                }
                let key =
                    TypeKey::Aggregate(TypeTag::Record, shape, words.clone().into_boxed_slice());
                let start = checked_u32(pools.type_extra.len(), "semantic_extra_overflow")?;
                (key, IrData::new(shape.raw(), start), words)
            }
            Type::Module(exports) => {
                let names = exports.keys().copied().collect::<Vec<_>>();
                let shape = self.intern_shape(pools, &names)?;
                let mut words = Vec::with_capacity(exports.len() * 2);
                for export in exports.values() {
                    match export {
                        ModuleExportType::Value { ty, optional } => {
                            words.push(u32::from(*optional) * MODULE_EXPORT_OPTIONAL);
                            words.push(self.intern_type(pools, ty)?.raw());
                        }
                        ModuleExportType::Proc { sig, optional } => {
                            words.push(1 | u32::from(*optional) * MODULE_EXPORT_OPTIONAL);
                            words.push(self.intern_signature(pools, sig)?.raw());
                        }
                        ModuleExportType::Pure { sig, optional } => {
                            words.push(2 | u32::from(*optional) * MODULE_EXPORT_OPTIONAL);
                            words.push(self.intern_signature(pools, sig)?.raw());
                        }
                    }
                }
                let key =
                    TypeKey::Aggregate(TypeTag::Module, shape, words.clone().into_boxed_slice());
                let start = checked_u32(pools.type_extra.len(), "semantic_extra_overflow")?;
                (key, IrData::new(shape.raw(), start), words)
            }
            Type::Result(ok, err) => {
                let ok = self.intern_type(pools, ok)?;
                let err = self.intern_type(pools, err)?;
                (
                    TypeKey::Pair(TypeTag::Result, ok, err),
                    IrData::new(ok.raw(), err.raw()),
                    Vec::new(),
                )
            }
            Type::Status => scalar(TypeTag::Status),
            Type::EnvPathList => scalar(TypeTag::EnvPathList),
            Type::Error => scalar(TypeTag::Error),
            Type::ErrorFamily(name) => named(TypeTag::ErrorFamily, *name),
            Type::ErrorVariant { family, variant } => (
                TypeKey::NamedPair(TypeTag::ErrorVariant, *family, *variant),
                IrData::new(family.symbol().raw(), variant.symbol().raw()),
                Vec::new(),
            ),
            Type::ErrorFacet(name) => named(TypeTag::ErrorFacet, *name),
            Type::ProcessError => scalar(TypeTag::ProcessError),
            Type::Pure => scalar(TypeTag::Pure),
            Type::Proc => scalar(TypeTag::Proc),
            Type::Command => scalar(TypeTag::Command),
            Type::ProcessHandle => scalar(TypeTag::ProcessHandle),
            Type::Unit => scalar(TypeTag::Unit),
            Type::Tag(name) => named(TypeTag::Tag, *name),
            Type::Optional(inner) => self.unary(pools, TypeTag::Optional, inner)?,
        };
        if let Some(id) = self.types.get(&key) {
            return Ok(*id);
        }
        let id = TypeId::new(pools.type_tags.len())?;
        let tag = match &key {
            TypeKey::Scalar(tag)
            | TypeKey::Unary(tag, _)
            | TypeKey::Pair(tag, _, _)
            | TypeKey::Named(tag, _)
            | TypeKey::NamedPair(tag, _, _)
            | TypeKey::Aggregate(tag, _, _) => *tag,
        };
        pools.type_tags.push(tag);
        pools.type_data.push(data);
        pools.type_extra.extend(extra);
        self.types.insert(key, id);
        Ok(id)
    }

    pub(super) fn intern_signature(
        &mut self,
        pools: &mut SemanticPools,
        signature: &CallableType,
    ) -> Result<SignatureId, IrBuildError> {
        let return_type = self.intern_type(pools, &signature.return_ty)?;
        let mut params = Vec::with_capacity(signature.params.len());
        for param in &signature.params {
            let mut flags = 0;
            if param.defaulted {
                flags |= PARAM_DEFAULTED;
            }
            if param.rest {
                flags |= PARAM_REST;
            }
            params.push(SignatureParamKey {
                name: param.name,
                ty: self.intern_type(pools, &param.ty)?,
                flags,
            });
        }
        let effects = normalized_effects(signature.effects.as_deref());
        self.intern_signature_key(pools, SignatureKey {
            return_type,
            effects,
            params: params.into_boxed_slice(),
        })
    }

    pub(super) fn intern_signature_parts(
        &mut self,
        pools: &mut SemanticPools,
        params: &[(Name, TypeId, u32)],
        return_type: TypeId,
        effects: Option<&[Effect]>,
    ) -> Result<SignatureId, IrBuildError> {
        let params = params
            .iter()
            .map(|(name, ty, flags)| SignatureParamKey {
                name: *name,
                ty: *ty,
                flags: *flags,
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let effects = normalized_effects(effects);
        self.intern_signature_key(pools, SignatureKey {
            return_type,
            effects,
            params,
        })
    }

    fn unary(
        &mut self,
        pools: &mut SemanticPools,
        tag: TypeTag,
        inner: &Type,
    ) -> Result<(TypeKey, IrData, Vec<u32>), IrBuildError> {
        let inner = self.intern_type(pools, inner)?;
        Ok((
            TypeKey::Unary(tag, inner),
            IrData::new(inner.raw(), 0),
            Vec::new(),
        ))
    }

    fn intern_signature_key(
        &mut self,
        pools: &mut SemanticPools,
        key: SignatureKey,
    ) -> Result<SignatureId, IrBuildError> {
        if let Some(id) = self.signatures.get(&key) {
            return Ok(*id);
        }
        let id = SignatureId::new(pools.signature_data.len())?;
        let mut words = Vec::with_capacity(
            3 + key.effects.as_ref().map_or(0, |effects| effects.len())
                + key.params.len() * 3,
        );
        words.push(key.return_type.raw());
        match &key.effects {
            None => words.push(IR_NONE),
            Some(effects) => {
                words.push(checked_u32(effects.len(), "effect_count_overflow")?);
            }
        }
        words.push(checked_u32(key.params.len(), "parameter_count_overflow")?);
        if let Some(effects) = &key.effects {
            words.extend(effects.iter().map(|effect| *effect as u32));
        }
        for param in &key.params {
            words.push(param.name.symbol().raw());
            words.push(param.ty.raw());
            words.push(param.flags);
        }
        let start = checked_u32(pools.signature_extra.len(), "semantic_extra_overflow")?;
        let len = checked_u32(words.len(), "semantic_extra_overflow")?;
        pools.signature_data.push(IrData::from_range(IrRange::new(start, len)));
        pools.signature_extra.extend(words);
        self.signatures.insert(key, id);
        Ok(id)
    }

    fn intern_shape(
        &mut self,
        pools: &mut SemanticPools,
        fields: &[Name],
    ) -> Result<ShapeId, IrBuildError> {
        if let Some(id) = self.shapes.get(fields) {
            return Ok(*id);
        }
        let id = ShapeId::new(pools.shapes.len())?;
        let start = checked_u32(pools.shape_fields.len(), "shape_field_overflow")?;
        let len = checked_u32(fields.len(), "shape_field_overflow")?;
        pools.shape_fields.extend_from_slice(fields);
        pools.shapes.push(IrRange::new(start, len));
        self.shapes.insert(fields.into(), id);
        Ok(id)
    }
}

fn scalar(tag: TypeTag) -> (TypeKey, IrData, Vec<u32>) {
    (TypeKey::Scalar(tag), IrData::ZERO, Vec::new())
}

fn named(tag: TypeTag, name: Name) -> (TypeKey, IrData, Vec<u32>) {
    (
        TypeKey::Named(tag, name),
        IrData::new(name.symbol().raw(), 0),
        Vec::new(),
    )
}

fn normalized_effects(effects: Option<&[Effect]>) -> Option<Box<[EffectCode]>> {
    effects.map(|effects| {
        let mut effects = effects.iter().map(EffectCode::from).collect::<Vec<_>>();
        effects.sort_unstable();
        effects.dedup();
        effects.into_boxed_slice()
    })
}

fn checked_u32(value: usize, construct: &'static str) -> Result<u32, IrBuildError> {
    u32::try_from(value).map_err(|_| IrBuildError::format(construct, None, 0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sema::types::{CallableParamType, CallableType};
    use std::collections::BTreeMap;

    fn callable() -> CallableType {
        CallableType {
            params: vec![
                CallableParamType {
                    name: Name::intern("path"),
                    ty: Type::Path,
                    defaulted: false,
                    rest: false,
                },
                CallableParamType {
                    name: Name::intern("flags"),
                    ty: Type::List(Box::new(Type::Str)),
                    defaulted: true,
                    rest: false,
                },
            ],
            return_ty: Box::new(Type::Result(
                Box::new(Type::Int),
                Box::new(Type::Error),
            )),
            effects: Some(vec![Effect::Fs, Effect::Error]),
        }
    }

    #[test]
    fn equal_types_signatures_and_shapes_share_compact_ids() {
        let mut pools = SemanticPools::default();
        let mut builder = SemanticPoolBuilder::default();
        let fields = BTreeMap::from([
            (Name::intern("count"), Type::Int),
            (Name::intern("name"), Type::Str),
        ]);
        let record = Type::Record(fields.clone());
        let first = builder.intern_type(&mut pools, &record).unwrap();
        let second = builder
            .intern_type(&mut pools, &Type::Record(fields))
            .unwrap();
        assert_eq!(first, second);

        let signature = callable();
        let first_signature = builder
            .intern_signature(&mut pools, &signature)
            .unwrap();
        let second_signature = builder
            .intern_signature(&mut pools, &signature)
            .unwrap();
        assert_eq!(first_signature, second_signature);
        let mut reordered_effects = signature.clone();
        reordered_effects.effects = Some(vec![Effect::Error, Effect::Fs, Effect::Fs]);
        assert_eq!(
            builder
                .intern_signature(&mut pools, &reordered_effects)
                .unwrap(),
            first_signature
        );

        let module = Type::Module(BTreeMap::from([
            (
                Name::intern("count"),
                ModuleExportType::Value {
                    ty: Type::Int,
                    optional: false,
                },
            ),
            (
                Name::intern("name"),
                ModuleExportType::Pure {
                    sig: callable(),
                    optional: false,
                },
            ),
        ]));
        let module_id = builder.intern_type(&mut pools, &module).unwrap();
        let record_shape = ShapeId::from_raw(pools.type_data[first.index()].lhs).unwrap();
        let module_shape = ShapeId::from_raw(pools.type_data[module_id.index()].lhs).unwrap();
        assert_eq!(record_shape, module_shape);
        assert_eq!(pools.shape_count(), 1);
        pools.verify().unwrap();
    }

    #[test]
    fn compact_types_render_like_owned_semantic_types() {
        let mut pools = SemanticPools::default();
        let mut builder = SemanticPoolBuilder::default();
        let types = [
            Type::List(Box::new(Type::Optional(Box::new(Type::Path)))),
            Type::Result(Box::new(Type::Int), Box::new(Type::ProcessError)),
            Type::ErrorVariant {
                family: Name::intern("BuildError"),
                variant: Name::intern("Failed"),
            },
            Type::Record(BTreeMap::from([(Name::intern("value"), Type::Str)])),
        ];
        for ty in types {
            let id = builder.intern_type(&mut pools, &ty).unwrap();
            assert_eq!(pools.display_type(id).unwrap(), ty.to_string());
        }
        pools.verify().unwrap();
    }

    #[test]
    fn recovery_types_never_become_executable_facts() {
        let mut pools = SemanticPools::default();
        let mut builder = SemanticPoolBuilder::default();
        for ty in [Type::Unknown, Type::Invalid] {
            let error = builder.intern_type(&mut pools, &ty).unwrap_err();
            assert_eq!(error.construct, "recovery_type");
        }
        assert_eq!(pools.type_count(), 0);
    }

    #[test]
    fn semantic_rewind_removes_rows_and_canonical_entries() {
        let mut pools = SemanticPools::default();
        let mut builder = SemanticPoolBuilder::default();
        let int = builder.intern_type(&mut pools, &Type::Int).unwrap();
        let checkpoint = builder.checkpoint(&pools);
        let list = builder
            .intern_type(&mut pools, &Type::List(Box::new(Type::Int)))
            .unwrap();
        assert_ne!(int, list);
        builder.rewind(&mut pools, checkpoint);
        assert_eq!(pools.type_count(), 1);
        let rebuilt = builder
            .intern_type(&mut pools, &Type::List(Box::new(Type::Int)))
            .unwrap();
        assert_eq!(rebuilt, list);
        pools.verify().unwrap();
    }

    #[test]
    fn malformed_semantic_ids_and_ranges_are_rejected() {
        let mut pools = SemanticPools::default();
        let mut builder = SemanticPoolBuilder::default();
        let list = builder
            .intern_type(&mut pools, &Type::List(Box::new(Type::Int)))
            .unwrap();
        let signature = builder
            .intern_signature(&mut pools, &callable())
            .unwrap();
        builder
            .intern_type(
                &mut pools,
                &Type::Record(BTreeMap::from([(Name::intern("value"), Type::Int)])),
            )
            .unwrap();
        pools.verify().unwrap();

        let mut bad_type = pools.clone();
        bad_type.type_data[list.index()].lhs = u32::MAX;
        assert!(bad_type.verify().is_err());

        let mut bad_signature = pools.clone();
        let range = bad_signature.signature_data[signature.index()].range();
        bad_signature.signature_extra[range.start as usize] = u32::MAX;
        assert!(bad_signature.verify().is_err());

        let mut bad_shape = pools;
        bad_shape.shapes[0].len = u32::MAX;
        assert!(bad_shape.verify().is_err());
    }
}
