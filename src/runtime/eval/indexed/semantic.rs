use super::{IR_NONE, IrBuildError, IrData, IrRange, IrVerifyError, ShapeId, SignatureId, TypeId};
use crate::sema::types::{CallableParamType, CallableType, ModuleExportType, Type};
use crate::sema::validated::{ValidatedType, Validation};
use crate::symbol::{Name, Symbol};
use crate::syntax::node::Effect;
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
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
    NetJob,
    FsRoot,
    Unit,
    Tag,
    Optional,
    UInt,
    ErasedRecord,
    DynamicModule,
    // A module type whose listed exports are the module's whole surface.
    // The payload is the same as `Module`.
    ExactModule,
    // A closed union. `lhs` is the member count and `rhs` the offset of the
    // member type ids in `type_extra`, in the order the union lists them.
    Union,
    // A callable with a checked signature. `lhs` is the signature id; the
    // signature has no defaulted or rest parameter.
    TypedProc,
    TypedPure,
    // A base type narrowed by a validation. `lhs` is the base type id and
    // `rhs` the validation's code.
    Validated,
}

impl TypeTag {
    fn has_no_payload(self) -> bool {
        matches!(
            self,
            Self::Any
                | Self::ErasedRecord
                | Self::DynamicModule
                | Self::Null
                | Self::Bool
                | Self::Int
                | Self::UInt
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
                | Self::NetJob
                | Self::FsRoot
                | Self::Unit
        )
    }

    fn has_one_type(self) -> bool {
        matches!(self, Self::List | Self::Stream | Self::Optional)
    }

    fn has_one_name(self) -> bool {
        matches!(self, Self::ErrorFamily | Self::ErrorFacet | Self::Tag)
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

    pub(super) fn signature_return_type(&self, id: SignatureId) -> Result<TypeId, IrVerifyError> {
        let payload = self.signature_payload(id)?;
        TypeId::from_raw(payload[0])
            .ok_or_else(|| IrVerifyError::new("signature return type id is invalid"))
    }

    pub(super) fn signature_param_count(&self, id: SignatureId) -> Result<usize, IrVerifyError> {
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

    pub(super) fn record_fields(&self, id: TypeId) -> Result<(&[Name], &[u32]), IrVerifyError> {
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

    /// The signature of a typed callable type.
    pub(super) fn typed_callable_signature(&self, id: TypeId) -> Result<SignatureId, IrVerifyError> {
        if !matches!(self.type_tag(id)?, TypeTag::TypedProc | TypeTag::TypedPure) {
            return Err(IrVerifyError::new("type id does not denote a typed callable"));
        }
        let signature = SignatureId::from_raw(self.type_data[id.index()].lhs)
            .ok_or_else(|| IrVerifyError::new("typed callable signature id is invalid"))?;
        if signature.index() >= self.signature_data.len() {
            return Err(IrVerifyError::new(
                "typed callable signature id is out of bounds",
            ));
        }
        Ok(signature)
    }

    pub(super) fn typed_callable_is_pure(&self, id: TypeId) -> Result<bool, IrVerifyError> {
        self.typed_callable_signature(id)?;
        Ok(self.type_tag(id)? == TypeTag::TypedPure)
    }

    /// The member type ids of a union, in the order the union lists them.
    fn union_members(&self, id: TypeId) -> Result<&[u32], IrVerifyError> {
        if self.type_tag(id)? != TypeTag::Union {
            return Err(IrVerifyError::new("type id does not denote a union"));
        }
        let data = self.type_data[id.index()];
        let start = data.rhs as usize;
        let end = start
            .checked_add(data.lhs as usize)
            .ok_or_else(|| IrVerifyError::new("union type payload overflows"))?;
        self.type_extra
            .get(start..end)
            .ok_or_else(|| IrVerifyError::new("union type payload is out of bounds"))
    }

    /// Rewrites the first union in the pool to claim a single member, as a
    /// corrupted program would, and reports whether the pool has a union.
    #[cfg(test)]
    pub(super) fn truncate_first_union_for_test(&mut self) -> bool {
        let Some(index) = self.type_tags.iter().position(|tag| *tag == TypeTag::Union) else {
            return false;
        };
        self.type_data[index].lhs = 1;
        true
    }

    /// Marks the first parameter of the first typed callable's signature as
    /// defaulted, as a corrupted program would, and reports whether the pool
    /// has a typed callable with a parameter.
    #[cfg(test)]
    pub(super) fn default_first_typed_callable_param_for_test(&mut self) -> bool {
        let Some(index) = self
            .type_tags
            .iter()
            .position(|tag| matches!(tag, TypeTag::TypedProc | TypeTag::TypedPure))
        else {
            return false;
        };
        let Some(signature) = SignatureId::from_raw(self.type_data[index].lhs) else {
            return false;
        };
        let range = self.signature_data[signature.index()];
        let Some(bounds) = range.range().bounds(self.signature_extra.len()) else {
            return false;
        };
        let payload = &mut self.signature_extra[bounds];
        let Ok(effects) = signature_effect_count(payload) else {
            return false;
        };
        if payload[2] == 0 {
            return false;
        }
        payload[3 + effects + 2] |= PARAM_DEFAULTED;
        true
    }

    /// Rewrites the first validated type in the pool as a corrupted program
    /// would: to name no validation, or (`rebase`) to validate its own
    /// element type instead of its base. Reports whether the pool has one.
    #[cfg(test)]
    pub(super) fn corrupt_first_validated_for_test(&mut self, rebase: bool) -> bool {
        let Some(index) = self
            .type_tags
            .iter()
            .position(|tag| *tag == TypeTag::Validated)
        else {
            return false;
        };
        if rebase {
            let base = TypeId::from_raw(self.type_data[index].lhs).expect("a base type id");
            self.type_data[index].lhs = self.type_data[base.index()].lhs;
        } else {
            self.type_data[index].rhs = 0;
        }
        true
    }

    pub(super) fn display_type(&self, id: TypeId) -> Result<String, IrVerifyError> {
        self.display_type_inner(id, 0)
    }

    pub(super) fn to_type(&self, id: TypeId) -> Result<Type, IrVerifyError> {
        self.to_type_inner(id, 0)
    }

    fn to_type_inner(&self, id: TypeId, depth: usize) -> Result<Type, IrVerifyError> {
        if depth > self.type_tags.len() {
            return Err(IrVerifyError::new("type graph contains a cycle"));
        }
        let tag = self.type_tag(id)?;
        let data = self.type_data[id.index()];
        let child = |raw: u32| {
            TypeId::from_raw(raw)
                .ok_or_else(|| IrVerifyError::new("type child id is invalid"))
                .and_then(|id| self.to_type_inner(id, depth + 1))
        };
        Ok(match tag {
            TypeTag::Any => Type::Any,
            TypeTag::ErasedRecord => Type::ErasedRecord,
            TypeTag::DynamicModule => Type::DynamicModule,
            TypeTag::Null => Type::Null,
            TypeTag::Bool => Type::Bool,
            TypeTag::Int => Type::Int,
            TypeTag::UInt => Type::UInt,
            TypeTag::Float => Type::Float,
            TypeTag::Duration => Type::Duration,
            TypeTag::Str => Type::Str,
            TypeTag::Bytes => Type::Bytes,
            TypeTag::Digest => Type::Digest,
            TypeTag::Regex => Type::Regex,
            TypeTag::Path => Type::Path,
            TypeTag::List => Type::List(Box::new(child(data.lhs)?)),
            TypeTag::Map => Type::Map(Box::new(child(data.lhs)?), Box::new(child(data.rhs)?)),
            TypeTag::Stream => Type::Stream(Box::new(child(data.lhs)?)),
            TypeTag::Record => {
                let (names, raw_types) = self.record_fields(id)?;
                let mut fields = BTreeMap::new();
                for (name, raw) in names.iter().copied().zip(raw_types.iter().copied()) {
                    fields.insert(name, child(raw)?);
                }
                Type::Record(fields)
            }
            TypeTag::Module | TypeTag::ExactModule => {
                let shape = ShapeId::from_raw(data.lhs)
                    .ok_or_else(|| IrVerifyError::new("module shape id is invalid"))?;
                let names = self.shape_fields(shape)?;
                let start = data.rhs as usize;
                let len = names
                    .len()
                    .checked_mul(2)
                    .ok_or_else(|| IrVerifyError::new("module payload length overflows"))?;
                let exports = self
                    .type_extra
                    .get(start..start + len)
                    .ok_or_else(|| IrVerifyError::new("module payload is out of bounds"))?;
                let mut fields = BTreeMap::new();
                for (name, export) in names.iter().copied().zip(exports.as_chunks::<2>().0) {
                    let optional = export[0] & MODULE_EXPORT_OPTIONAL != 0;
                    let value = match export[0] & 0b11 {
                        0 => ModuleExportType::Value {
                            ty: child(export[1])?,
                            optional,
                        },
                        1 => ModuleExportType::Proc {
                            sig: self.to_signature(
                                SignatureId::from_raw(export[1]).ok_or_else(|| {
                                    IrVerifyError::new("module proc signature id is invalid")
                                })?,
                                depth + 1,
                            )?,
                            optional,
                        },
                        2 => ModuleExportType::Pure {
                            sig: self.to_signature(
                                SignatureId::from_raw(export[1]).ok_or_else(|| {
                                    IrVerifyError::new("module pure signature id is invalid")
                                })?,
                                depth + 1,
                            )?,
                            optional,
                        },
                        _ => {
                            return Err(IrVerifyError::new("module export kind is invalid"));
                        }
                    };
                    fields.insert(name, value);
                }
                Type::Module(std::sync::Arc::new(crate::sema::types::ModuleType {
                    exports: fields,
                    exact: tag == TypeTag::ExactModule,
                }))
            }
            TypeTag::Result => Type::Result(Box::new(child(data.lhs)?), Box::new(child(data.rhs)?)),
            TypeTag::Status => Type::Status,
            TypeTag::EnvPathList => Type::EnvPathList,
            TypeTag::Error => Type::Error,
            TypeTag::ErrorFamily => {
                Type::ErrorFamily(Name::from_symbol(Symbol::from_raw(data.lhs)))
            }
            TypeTag::ErrorVariant => Type::ErrorVariant {
                family: Name::from_symbol(Symbol::from_raw(data.lhs)),
                variant: Name::from_symbol(Symbol::from_raw(data.rhs)),
            },
            TypeTag::ErrorFacet => Type::ErrorFacet(Name::from_symbol(Symbol::from_raw(data.lhs))),
            TypeTag::ProcessError => Type::ProcessError,
            TypeTag::Pure => Type::Pure,
            TypeTag::Proc => Type::Proc,
            TypeTag::TypedProc | TypeTag::TypedPure => {
                Type::Callable(std::sync::Arc::new(crate::sema::types::TypedCallable {
                    pure: tag == TypeTag::TypedPure,
                    sig: self.to_signature(self.typed_callable_signature(id)?, depth + 1)?,
                }))
            }
            TypeTag::Command => Type::Command,
            TypeTag::ProcessHandle => Type::ProcessHandle,
            TypeTag::NetJob => Type::NetJob,
            TypeTag::FsRoot => Type::FsRoot,
            TypeTag::Unit => Type::Unit,
            TypeTag::Tag => Type::Tag(Name::from_symbol(Symbol::from_raw(data.lhs))),
            TypeTag::Optional => Type::Optional(Box::new(child(data.lhs)?)),
            TypeTag::Validated => {
                let validation = Validation::from_code(data.rhs)
                    .ok_or_else(|| IrVerifyError::new("validated type names no validation"))?;
                Type::Validated(Box::new(
                    ValidatedType::new(validation, child(data.lhs)?).map_err(|_| {
                        IrVerifyError::new("validated type has a base its validation rejects")
                    })?,
                ))
            }
            TypeTag::Union => Type::Union(
                self.union_members(id)?
                    .iter()
                    .map(|raw| child(*raw))
                    .collect::<Result<_, _>>()?,
            ),
        })
    }

    fn to_signature(&self, id: SignatureId, depth: usize) -> Result<CallableType, IrVerifyError> {
        if depth > self.type_tags.len() + self.signature_data.len() {
            return Err(IrVerifyError::new("semantic graph contains a cycle"));
        }
        let payload = self.signature_payload(id)?;
        let effect_count = signature_effect_count(payload)?;
        let effects = if payload[1] == IR_NONE {
            None
        } else {
            let mut effects = Vec::with_capacity(effect_count);
            for raw in &payload[3..3 + effect_count] {
                effects.push(match *raw {
                    value if value == EffectCode::Fs as u32 => Effect::Fs,
                    value if value == EffectCode::Net as u32 => Effect::Net,
                    value if value == EffectCode::Process as u32 => Effect::Process,
                    value if value == EffectCode::Env as u32 => Effect::Env,
                    value if value == EffectCode::Time as u32 => Effect::Time,
                    value if value == EffectCode::Error as u32 => Effect::Error,
                    value if value == EffectCode::Io as u32 => Effect::Io,
                    _ => return Err(IrVerifyError::new("signature effect is invalid")),
                });
            }
            Some(effects)
        };
        let mut params = Vec::with_capacity(payload[2] as usize);
        for raw in payload[3 + effect_count..].as_chunks::<3>().0 {
            params.push(CallableParamType {
                name: Name::from_symbol(Symbol::from_raw(raw[0])),
                ty: self.to_type_inner(
                    TypeId::from_raw(raw[1])
                        .ok_or_else(|| IrVerifyError::new("parameter type id is invalid"))?,
                    depth + 1,
                )?,
                defaulted: raw[2] & PARAM_DEFAULTED != 0,
                rest: raw[2] & PARAM_REST != 0,
            });
        }
        Ok(CallableType {
            params,
            return_ty: Box::new(
                self.to_type_inner(
                    TypeId::from_raw(payload[0])
                        .ok_or_else(|| IrVerifyError::new("return type id is invalid"))?,
                    depth + 1,
                )?,
            ),
            effects,
        })
    }

    fn display_type_inner(&self, id: TypeId, depth: usize) -> Result<String, IrVerifyError> {
        if depth > self.type_tags.len() {
            return Err(IrVerifyError::new("type graph contains a cycle"));
        }
        let tag = self.type_tag(id)?;
        let data = self.type_data[id.index()];
        let scalar = match tag {
            TypeTag::Any => Some("Any"),
            TypeTag::ErasedRecord => Some("Record"),
            TypeTag::DynamicModule => Some("Module"),
            TypeTag::Null => Some("Null"),
            TypeTag::Bool => Some("Bool"),
            TypeTag::Int => Some("Int"),
            TypeTag::UInt => Some("UInt"),
            TypeTag::Float => Some("Float"),
            TypeTag::Duration => Some("Duration"),
            TypeTag::Str => Some("Str"),
            TypeTag::Bytes => Some("Bytes"),
            TypeTag::Digest => Some("Digest"),
            TypeTag::Regex => Some("Regex"),
            TypeTag::Path => Some("Path"),
            TypeTag::Record => Some("Record"),
            TypeTag::Module | TypeTag::ExactModule => Some("Module"),
            TypeTag::Status => Some("Status"),
            TypeTag::EnvPathList => Some("EnvPathList"),
            TypeTag::Error => Some("Error"),
            TypeTag::ProcessError => Some("ProcessError"),
            TypeTag::Pure => Some("Pure"),
            TypeTag::Proc => Some("Proc"),
            TypeTag::Command => Some("Command"),
            TypeTag::ProcessHandle => Some("ProcessHandle"),
            TypeTag::NetJob => Some("NetJob"),
            TypeTag::FsRoot => Some("FsRoot"),
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
                TypeTag::Stream => format!("Stream[{inner}]"),
                TypeTag::Optional => format!("{inner}?"),
                _ => unreachable!("one-type tags are exhaustive"),
            });
        }
        if tag.has_one_name() {
            return Ok(Name::from_symbol(Symbol::from_raw(data.lhs)).to_string());
        }
        match tag {
            TypeTag::Map => {
                let key = TypeId::from_raw(data.lhs)
                    .ok_or_else(|| IrVerifyError::new("map key type id is invalid"))?;
                let value = TypeId::from_raw(data.rhs)
                    .ok_or_else(|| IrVerifyError::new("map value type id is invalid"))?;
                let key = self.display_type_inner(key, depth + 1)?;
                let value = self.display_type_inner(value, depth + 1)?;
                Ok(if key == "Str" {
                    format!("Map[{value}]")
                } else {
                    format!("Map[{key}, {value}]")
                })
            }
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
            TypeTag::Validated => Ok(self.to_type_inner(id, depth)?.to_string()),
            TypeTag::Union => {
                let mut members = Vec::new();
                for raw in self.union_members(id)? {
                    let member = TypeId::from_raw(*raw)
                        .ok_or_else(|| IrVerifyError::new("union member type id is invalid"))?;
                    members.push(self.display_type_inner(member, depth + 1)?);
                }
                Ok(format!("Union[{}]", members.join(", ")))
            }
            TypeTag::TypedProc | TypeTag::TypedPure => Ok(self.to_type_inner(id, depth)?.to_string()),
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
            for param in payload[3 + effects..].as_chunks::<3>().0 {
                verify_type_raw(self, param[1], None)?;
                if param[2] & !(PARAM_DEFAULTED | PARAM_REST) != 0 {
                    return Err(IrVerifyError::new("signature parameter flags are invalid"));
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
                TypeTag::Map | TypeTag::Result => {
                    verify_type_raw(self, data.lhs, Some(index))?;
                    verify_type_raw(self, data.rhs, Some(index))?;
                    if tag == TypeTag::Map {
                        let key = TypeId::from_raw(data.lhs).expect("verified key type id");
                        if !matches!(
                            self.type_tags[key.index()],
                            TypeTag::Any
                                | TypeTag::Str
                                | TypeTag::Int
                                | TypeTag::UInt
                                | TypeTag::Bool
                                | TypeTag::Bytes
                                | TypeTag::Path
                                | TypeTag::Duration
                        ) {
                            return Err(IrVerifyError::new(
                                "Map key type is not an ordered scalar domain",
                            ));
                        }
                    }
                }
                TypeTag::ErrorVariant => {}
                // A validated type is published only for a validation the
                // checker knows, over a base that validation applies to; the
                // runtime test of the type reads the base's representation.
                TypeTag::Validated => {
                    verify_type_raw(self, data.lhs, Some(index))?;
                    let id =
                        TypeId::new(index).map_err(|_| IrVerifyError::new("type id overflows"))?;
                    self.to_type(id)?;
                }
                // The checker never publishes a union it would have to
                // simplify, and the runtime tries members in order, so a
                // pool whose union has one member, repeats one, or lists a
                // type that accepts every value, `null`, or another union's
                // values was not produced by lowering.
                TypeTag::Union => {
                    let id =
                        TypeId::new(index).map_err(|_| IrVerifyError::new("type id overflows"))?;
                    let members = self.union_members(id)?;
                    if members.len() < 2 {
                        return Err(IrVerifyError::new("union type has fewer than two members"));
                    }
                    for (position, raw) in members.iter().enumerate() {
                        let member = verify_type_raw(self, *raw, Some(index))?;
                        if matches!(
                            self.type_tags[member.index()],
                            TypeTag::Any
                                | TypeTag::Null
                                | TypeTag::Optional
                                | TypeTag::Stream
                                | TypeTag::Union
                        ) {
                            return Err(IrVerifyError::new(
                                "union member type cannot be a member of a union",
                            ));
                        }
                        if members[..position].contains(raw) {
                            return Err(IrVerifyError::new("union type repeats a member"));
                        }
                    }
                }
                TypeTag::Record => {
                    let id =
                        TypeId::new(index).map_err(|_| IrVerifyError::new("type id overflows"))?;
                    let (_, fields) = self.record_fields(id)?;
                    for raw in fields {
                        verify_type_raw(self, *raw, Some(index))?;
                    }
                }
                TypeTag::Module | TypeTag::ExactModule => {
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
                    let exports = self
                        .type_extra
                        .get(start..end)
                        .ok_or_else(|| IrVerifyError::new("module payload is out of bounds"))?;
                    for export in exports.as_chunks::<2>().0 {
                        let kind = export[0] & 0b11;
                        if export[0] & !(0b11 | MODULE_EXPORT_OPTIONAL) != 0 || kind > 2 {
                            return Err(IrVerifyError::new("module export flags are invalid"));
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
                // A call through a typed callable supplies exactly the
                // parameters its signature lists, so a signature that would
                // let one be omitted or collected was not produced by
                // lowering.
                TypeTag::TypedProc | TypeTag::TypedPure => {
                    let id =
                        TypeId::new(index).map_err(|_| IrVerifyError::new("type id overflows"))?;
                    let signature = self.typed_callable_signature(id)?;
                    if data.rhs != 0 {
                        return Err(IrVerifyError::new("typed callable type has invalid data"));
                    }
                    for param in 0..self.signature_param_count(signature)? {
                        if self.signature_param(signature, param)?.2 != 0 {
                            return Err(IrVerifyError::new(
                                "typed callable signature has a defaulted or rest parameter",
                            ));
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
    let id = TypeId::from_raw(raw).ok_or_else(|| IrVerifyError::new("type id is invalid"))?;
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
    /// Member type ids in the order the union lists them.
    Union(Box<[u32]>),
    Callable(TypeTag, SignatureId),
    /// A validation code and the base type it narrows.
    Validated(u32, TypeId),
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
                TypeKey::Aggregate(_, _, words) | TypeKey::Union(words) => {
                    words.len() * size_of::<u32>()
                }
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

    pub(super) fn rewind(&mut self, pools: &mut SemanticPools, checkpoint: SemanticCheckpoint) {
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
        self.shapes.retain(|_, id| id.index() < checkpoint.shapes);
    }

    pub(super) fn intern_type(
        &mut self,
        pools: &mut SemanticPools,
        ty: &Type,
    ) -> Result<TypeId, IrBuildError> {
        let (key, data, extra) = match ty {
            Type::Inference(_) => {
                return Err(IrBuildError::format("unresolved_type", None, 0, 0));
            }
            Type::BuiltinParameter(_) | Type::Unknown | Type::Invalid => {
                return Err(IrBuildError::format("recovery_type", None, 0, 0));
            }
            Type::Any => scalar(TypeTag::Any),
            Type::ErasedRecord => scalar(TypeTag::ErasedRecord),
            Type::Null => scalar(TypeTag::Null),
            Type::Bool => scalar(TypeTag::Bool),
            Type::Int => scalar(TypeTag::Int),
            Type::UInt => scalar(TypeTag::UInt),
            Type::Float => scalar(TypeTag::Float),
            Type::Duration => scalar(TypeTag::Duration),
            Type::Str => scalar(TypeTag::Str),
            Type::Bytes => scalar(TypeTag::Bytes),
            Type::Digest => scalar(TypeTag::Digest),
            Type::Regex => scalar(TypeTag::Regex),
            Type::Path => scalar(TypeTag::Path),
            Type::List(inner) => self.unary(pools, TypeTag::List, inner)?,
            Type::Map(key, value) => {
                let key = self.intern_type(pools, key)?;
                let value = self.intern_type(pools, value)?;
                (
                    TypeKey::Pair(TypeTag::Map, key, value),
                    IrData::new(key.raw(), value.raw()),
                    Vec::new(),
                )
            }
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
                            words.push(1 | (u32::from(*optional) * MODULE_EXPORT_OPTIONAL));
                            words.push(self.intern_signature(pools, sig)?.raw());
                        }
                        ModuleExportType::Pure { sig, optional } => {
                            words.push(2 | (u32::from(*optional) * MODULE_EXPORT_OPTIONAL));
                            words.push(self.intern_signature(pools, sig)?.raw());
                        }
                    }
                }
                let tag = if exports.exact {
                    TypeTag::ExactModule
                } else {
                    TypeTag::Module
                };
                let key = TypeKey::Aggregate(tag, shape, words.clone().into_boxed_slice());
                let start = checked_u32(pools.type_extra.len(), "semantic_extra_overflow")?;
                (key, IrData::new(shape.raw(), start), words)
            }
            Type::DynamicModule => scalar(TypeTag::DynamicModule),
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
            Type::NetJob => scalar(TypeTag::NetJob),
            Type::FsRoot => scalar(TypeTag::FsRoot),
            Type::Unit => scalar(TypeTag::Unit),
            Type::Tag(name) => named(TypeTag::Tag, *name),
            Type::Optional(inner) => self.unary(pools, TypeTag::Optional, inner)?,
            Type::Union(members) => {
                let mut words = Vec::with_capacity(members.len());
                for member in members {
                    words.push(self.intern_type(pools, member)?.raw());
                }
                let count = checked_u32(words.len(), "semantic_extra_overflow")?;
                let start = checked_u32(pools.type_extra.len(), "semantic_extra_overflow")?;
                (
                    TypeKey::Union(words.clone().into_boxed_slice()),
                    IrData::new(count, start),
                    words,
                )
            }
            Type::Validated(validated) => {
                let base = self.intern_type(pools, validated.base())?;
                let code = validated.validation().code();
                (
                    TypeKey::Validated(code, base),
                    IrData::new(base.raw(), code),
                    Vec::new(),
                )
            }
            Type::Callable(callable) => {
                let signature = self.intern_signature(pools, &callable.sig)?;
                let tag = if callable.pure {
                    TypeTag::TypedPure
                } else {
                    TypeTag::TypedProc
                };
                (
                    TypeKey::Callable(tag, signature),
                    IrData::new(signature.raw(), 0),
                    Vec::new(),
                )
            }
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
            TypeKey::Union(_) => TypeTag::Union,
            TypeKey::Callable(tag, _) => *tag,
            TypeKey::Validated(_, _) => TypeTag::Validated,
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
        self.intern_signature_key(
            pools,
            SignatureKey {
                return_type,
                effects,
                params: params.into_boxed_slice(),
            },
        )
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
        self.intern_signature_key(
            pools,
            SignatureKey {
                return_type,
                effects,
                params,
            },
        )
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
            3 + key.effects.as_ref().map_or(0, |effects| effects.len()) + key.params.len() * 3,
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
        pools
            .signature_data
            .push(IrData::from_range(IrRange::new(start, len)));
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
    #[test]
    fn typed_map_keys_semantic_pool_retains_both_types_and_rejects_float_keys() {
        let mut pools = super::SemanticPools::default();
        let mut builder = super::SemanticPoolBuilder::default();
        let float = builder
            .intern_type(&mut pools, &crate::sema::types::Type::Float)
            .unwrap();
        let map = crate::sema::types::Type::Map(
            Box::new(crate::sema::types::Type::UInt),
            Box::new(crate::sema::types::Type::Str),
        );
        let id = builder.intern_type(&mut pools, &map).unwrap();
        pools.verify().unwrap();
        assert_eq!(pools.to_type(id).unwrap(), map);
        assert_eq!(pools.display_type(id).unwrap(), "Map[UInt, Str]");
        pools.type_data[id.index()].lhs = float.raw();
        assert!(pools.verify().unwrap_err().message.contains("Map key type"));
    }

    use super::*;
    use crate::sema::types::{CallableParamType, CallableType};
    use std::collections::BTreeMap;

    fn module_type(
        exports: BTreeMap<Name, ModuleExportType>,
    ) -> std::sync::Arc<crate::sema::types::ModuleType> {
        std::sync::Arc::new(crate::sema::types::ModuleType::open(exports))
    }

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
            return_ty: Box::new(Type::Result(Box::new(Type::Int), Box::new(Type::Error))),
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
        let first_signature = builder.intern_signature(&mut pools, &signature).unwrap();
        let second_signature = builder.intern_signature(&mut pools, &signature).unwrap();
        assert_eq!(first_signature, second_signature);
        let mut reordered_effects = signature.clone();
        reordered_effects.effects = Some(vec![Effect::Error, Effect::Fs, Effect::Fs]);
        assert_eq!(
            builder
                .intern_signature(&mut pools, &reordered_effects)
                .unwrap(),
            first_signature
        );

        let module = Type::Module(module_type(BTreeMap::from([
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
        ])));
        let module_id = builder.intern_type(&mut pools, &module).unwrap();
        let record_shape = ShapeId::from_raw(pools.type_data[first.index()].lhs).unwrap();
        let module_shape = ShapeId::from_raw(pools.type_data[module_id.index()].lhs).unwrap();
        assert_eq!(record_shape, module_shape);
        assert_eq!(pools.shape_count(), 1);
        pools.verify().unwrap();
    }

    // Exactness is part of a module type's identity: it survives the pool,
    // and an exact payload is verified like an open one.
    #[test]
    fn exact_module_types_keep_their_exactness_and_are_verified() {
        use crate::sema::types::ModuleType;
        let exports = BTreeMap::from([(
            Name::intern("count"),
            ModuleExportType::Value {
                ty: Type::Int,
                optional: false,
            },
        )]);
        let open = Type::Module(std::sync::Arc::new(ModuleType::open(exports.clone())));
        let exact = Type::Module(std::sync::Arc::new(ModuleType::exact(exports)));
        let mut pools = SemanticPools::default();
        let mut builder = SemanticPoolBuilder::default();
        let open_id = builder.intern_type(&mut pools, &open).unwrap();
        let exact_id = builder.intern_type(&mut pools, &exact).unwrap();
        assert_ne!(open_id, exact_id);
        assert_eq!(pools.to_type(open_id).unwrap(), open);
        assert_eq!(pools.to_type(exact_id).unwrap(), exact);
        pools.verify().unwrap();

        let mut corrupted = pools.clone();
        let payload = corrupted.type_data[exact_id.index()].rhs as usize;
        corrupted.type_extra[payload] = 3;
        assert!(
            corrupted
                .verify()
                .unwrap_err()
                .message
                .contains("module export flags are invalid")
        );
    }

    #[test]
    fn erased_record_and_module_facts_remain_distinct_from_empty_shapes() {
        let mut pools = SemanticPools::default();
        let mut builder = SemanticPoolBuilder::default();
        let erased_record = builder
            .intern_type(&mut pools, &Type::ErasedRecord)
            .unwrap();
        let empty_record = builder
            .intern_type(&mut pools, &Type::Record(BTreeMap::new()))
            .unwrap();
        let dynamic_module = builder
            .intern_type(&mut pools, &Type::DynamicModule)
            .unwrap();
        let empty_module = builder
            .intern_type(
                &mut pools,
                &Type::Module(module_type(BTreeMap::new())),
            )
            .unwrap();
        assert_ne!(erased_record, empty_record);
        assert_ne!(dynamic_module, empty_module);
        assert_eq!(pools.to_type(erased_record).unwrap(), Type::ErasedRecord);
        assert_eq!(
            pools.to_type(empty_record).unwrap(),
            Type::Record(BTreeMap::new())
        );
        assert_eq!(pools.to_type(dynamic_module).unwrap(), Type::DynamicModule);
        assert_eq!(
            pools.to_type(empty_module).unwrap(),
            Type::Module(module_type(BTreeMap::new()))
        );
        pools.verify().unwrap();
    }

    #[test]
    fn compact_types_render_like_owned_semantic_types() {
        crate::symbol::SymbolOwner::new().with_current(|| {
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
        });
    }

    #[test]
    fn union_types_round_trip_in_member_order_and_reject_corruption() {
        let mut pools = SemanticPools::default();
        let mut builder = SemanticPoolBuilder::default();
        let words = Type::Union(vec![Type::Str, Type::Path]);
        let reversed = Type::Union(vec![Type::Path, Type::Str]);
        let id = builder
            .intern_type(&mut pools, &Type::List(Box::new(words.clone())))
            .unwrap();
        let union = builder.intern_type(&mut pools, &words).unwrap();
        let other = builder.intern_type(&mut pools, &reversed).unwrap();
        // Member order is part of the type the runtime tries members in.
        assert_ne!(union, other);
        assert_eq!(builder.intern_type(&mut pools, &words).unwrap(), union);
        assert_eq!(pools.to_type(union).unwrap(), words);
        assert_eq!(pools.to_type(other).unwrap(), reversed);
        assert_eq!(pools.display_type(id).unwrap(), "List[Union[Str, Path]]");
        pools.verify().unwrap();

        let data = pools.type_data[union.index()];
        let any = builder.intern_type(&mut pools, &Type::Any).unwrap();
        let optional = builder
            .intern_type(&mut pools, &Type::Optional(Box::new(Type::Str)))
            .unwrap();
        pools.verify().unwrap();

        let mut one_member = pools.clone();
        one_member.type_data[union.index()].lhs = 1;
        assert!(one_member.verify().is_err());

        let mut out_of_bounds = pools.clone();
        out_of_bounds.type_data[union.index()].lhs = u32::MAX;
        assert!(out_of_bounds.verify().is_err());

        let mut repeated = pools.clone();
        repeated.type_extra[data.rhs as usize + 1] = repeated.type_extra[data.rhs as usize];
        assert!(repeated.verify().is_err());

        // A member must be a type lowering could have listed, and must
        // precede the union that lists it.
        for member in [any.raw(), optional.raw(), other.raw(), union.raw()] {
            let mut bad_member = pools.clone();
            bad_member.type_extra[data.rhs as usize] = member;
            assert!(bad_member.verify().is_err());
        }
    }

    #[test]
    fn validated_types_round_trip_and_reject_corruption() {
        let mut pools = SemanticPools::default();
        let mut builder = SemanticPoolBuilder::default();
        let names = Type::non_empty(Type::Str);
        let list = builder
            .intern_type(&mut pools, &Type::List(Box::new(Type::Str)))
            .unwrap();
        let id = builder.intern_type(&mut pools, &names).unwrap();
        // The validated type and its base are different types to the runtime.
        assert_ne!(id, list);
        assert_eq!(builder.intern_type(&mut pools, &names).unwrap(), id);
        assert_eq!(pools.to_type(id).unwrap(), names);
        assert_eq!(pools.display_type(id).unwrap(), "NonEmpty[Str]");
        pools.verify().unwrap();

        // A validation the checker does not know.
        let mut unknown_validation = pools.clone();
        unknown_validation.type_data[id.index()].rhs = 0;
        assert!(unknown_validation.verify().is_err());
        unknown_validation.type_data[id.index()].rhs = u32::MAX;
        assert!(unknown_validation.verify().is_err());

        // A base the validation does not apply to: `Str` is not a list.
        let text = builder.intern_type(&mut pools, &Type::Str).unwrap();
        let mut wrong_base = pools.clone();
        wrong_base.type_data[id.index()].lhs = text.raw();
        assert!(wrong_base.verify().is_err());

        // A base that does not precede the type, or is no type at all.
        for base in [id.raw(), 0, u32::MAX] {
            let mut bad_base = pools.clone();
            bad_base.type_data[id.index()].lhs = base;
            assert!(bad_base.verify().is_err());
        }

        // A validation over a scalar base round-trips the same way, and its
        // row cannot be read as the validation of another base.
        let rel = Type::rel_path();
        let rel_id = builder.intern_type(&mut pools, &rel).unwrap();
        pools.verify().unwrap();
        assert_eq!(pools.to_type(rel_id).unwrap(), rel);
        assert_eq!(pools.display_type(rel_id).unwrap(), "RelPath");
        assert_ne!(rel_id, builder.intern_type(&mut pools, &Type::Path).unwrap());
        let mut swapped = pools.clone();
        swapped.type_data[rel_id.index()].rhs = Validation::NonEmpty.code();
        assert!(swapped.verify().is_err());
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
        let signature = builder.intern_signature(&mut pools, &callable()).unwrap();
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
