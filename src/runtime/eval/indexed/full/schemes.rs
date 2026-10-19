use super::schema::{
    FullSchemeSchemaComponentTag, FullSchemeTypeTag, FullSchemeValidationTag,
};
use super::{
    FullBuilder, FullStore, IrBuildError, IrFunctionId, IrRange, IR_NONE, SignatureId, TypeId,
};
use crate::sema::constants::{SchemaComponent, SchemaExpectation};
use crate::sema::schemes::{
    CallInstantiation, RowEvidence, SchemeCallableKind, SchemeRowTail,
    SchemeType, SchemeTypeParameterId, SchemeRowParameterId, TypeScheme,
};
use crate::sema::types::Type;
use crate::sema::validated::Validation;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(super) struct FullSchemeId(u32);

impl FullSchemeId {
    pub(super) fn new(index: usize) -> Result<Self, IrBuildError> {
        let raw = u32::try_from(index)
            .ok()
            .filter(|raw| *raw != IR_NONE)
            .ok_or_else(|| IrBuildError::format("scheme_overflow", None, 0, 0))?;
        Ok(Self(raw))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }

    pub(super) fn from_raw(raw: u32) -> Result<Self, super::IrVerifyError> {
        (raw != IR_NONE)
            .then_some(Self(raw))
            .ok_or_else(|| super::IrVerifyError::new("full scheme id is invalid"))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(super) struct SchemeTypeId(u32);

impl SchemeTypeId {
    pub(super) fn new(index: usize) -> Result<Self, IrBuildError> {
        let raw = u32::try_from(index)
            .ok()
            .filter(|raw| *raw != IR_NONE)
            .ok_or_else(|| IrBuildError::format("scheme_type_overflow", None, 0, 0))?;
        Ok(Self(raw))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }

    pub(super) fn raw(self) -> u32 {
        self.0
    }

    pub(super) fn from_raw(raw: u32) -> Result<Self, super::IrVerifyError> {
        (raw != IR_NONE)
            .then_some(Self(raw))
            .ok_or_else(|| super::IrVerifyError::new("scheme type id is invalid"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C, u8)]
pub(super) enum FullCallableContract {
    Ground(SignatureId),
    Scheme(FullSchemeId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum FullSchemeCallableKind {
    Pure,
    Proc,
    Stream,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum FullSchemeBodyFactKind {
    Expression,
    Tail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullScheme {
    pub(super) type_parameters: IrRange,
    pub(super) row_parameters: IrRange,
    pub(super) params: IrRange,
    pub(super) return_type: SchemeTypeId,
    pub(super) return_schema: Option<FullSchemeSchemaExpectationId>,
    pub(super) effects: IrRange,
    pub(super) body_facts: IrRange,
    pub(super) callable_kind: FullSchemeCallableKind,
    pub(super) has_effects: bool,
    pub(super) explicit_return: bool,
    pub(super) inferred_effects: bool,
    pub(super) reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeTypeParameter {
    pub(super) id: u32,
    pub(super) binder: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeRowParameter {
    pub(super) id: u32,
    pub(super) binder: u32,
    pub(super) lacks: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeParam {
    pub(super) ty: SchemeTypeId,
    pub(super) schema: Option<FullSchemeSchemaExpectationId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeTypeNode {
    pub(super) first: u32,
    pub(super) second: u32,
    pub(super) children: IrRange,
    pub(super) tag: FullSchemeTypeTag,
    pub(super) reserved: [u8; 3],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeField {
    pub(super) name: u32,
    pub(super) ty: SchemeTypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeValidation {
    pub(super) name: u32,
    pub(super) low: i64,
    pub(super) high: i64,
    pub(super) tag: FullSchemeValidationTag,
    pub(super) reserved: [u8; 7],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(super) struct FullSchemeSchemaExpectationId(u32);

impl FullSchemeSchemaExpectationId {
    fn new(index: usize) -> Result<Self, IrBuildError> {
        let raw = u32::try_from(index)
            .ok()
            .filter(|raw| *raw != IR_NONE)
            .ok_or_else(|| IrBuildError::format("scheme_schema_overflow", None, 0, 0))?;
        Ok(Self(raw))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeSchemaExpectation {
    pub(super) instances: IrRange,
    pub(super) children: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeSchemaInstance {
    pub(super) definition: u32,
    pub(super) arguments: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeSchemaChild {
    pub(super) component: FullSchemeSchemaComponentTag,
    pub(super) reserved: [u8; 3],
    pub(super) name: u32,
    pub(super) expectation: FullSchemeSchemaExpectationId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullCallInstantiation {
    pub(super) instruction: u32,
    pub(super) type_arguments: IrRange,
    pub(super) row_arguments: IrRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullTypeArgument {
    pub(super) binder: u32,
    pub(super) ty: SchemeTypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullRowArgument {
    pub(super) binder: u32,
    pub(super) fields: IrRange,
    pub(super) tail: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullRowEvidenceField {
    pub(super) name: u32,
    pub(super) ty: SchemeTypeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullSchemeBodyFact {
    pub(super) instruction: u32,
    pub(super) function: IrFunctionId,
    pub(super) ty: SchemeTypeId,
    pub(super) kind: FullSchemeBodyFactKind,
    pub(super) reserved: [u8; 3],
}

impl FullStore {
    pub(super) fn scheme(&self, id: FullSchemeId) -> Result<&FullScheme, super::IrVerifyError> {
        self.schemes
            .get(id.index())
            .ok_or_else(|| super::IrVerifyError::new("full scheme id is out of bounds"))
    }

    pub(super) fn scheme_type(
        &self,
        id: SchemeTypeId,
    ) -> Result<&FullSchemeTypeNode, super::IrVerifyError> {
        self.scheme_types
            .get(id.index())
            .ok_or_else(|| super::IrVerifyError::new("scheme type id is out of bounds"))
    }
}

impl FullBuilder {
    pub(super) fn intern_scheme(
        &mut self,
        scheme: &TypeScheme,
    ) -> Result<FullSchemeId, IrBuildError> {
        let type_parameters = scheme
            .type_parameters()
            .iter()
            .copied()
            .enumerate()
            .map(|(binder, id)| {
                Ok((id, u32::try_from(binder).map_err(|_| {
                    IrBuildError::format("scheme_type_binder_overflow", None, 0, 0)
                })?))
            })
            .collect::<Result<BTreeMap<_, _>, IrBuildError>>()?;
        let row_parameters = scheme
            .row_parameters()
            .iter()
            .enumerate()
            .map(|(binder, parameter)| {
                Ok((
                    parameter.id,
                    u32::try_from(binder).map_err(|_| {
                        IrBuildError::format("scheme_row_binder_overflow", None, 0, 0)
                    })?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, IrBuildError>>()?;

        let type_parameters_start = self.store.scheme_type_parameters.len();
        for (binder, id) in scheme.type_parameters().iter().copied().enumerate() {
            self.store.scheme_type_parameters.push(FullSchemeTypeParameter {
                id: id.0,
                binder: u32::try_from(binder).map_err(|_| {
                    IrBuildError::format("scheme_type_binder_overflow", None, 0, 0)
                })?,
            });
        }
        let type_parameters_range = super::table_range(
            type_parameters_start,
            self.store.scheme_type_parameters.len(),
        )?;

        let row_parameters_start = self.store.scheme_row_parameters.len();
        for (binder, parameter) in scheme.row_parameters().iter().enumerate() {
            let lacks_start = self.store.scheme_row_lacks.len();
            let mut lacks = parameter
                .lacks
                .iter()
                .copied()
                .collect::<Vec<_>>();
            lacks.sort_by(|left, right| left.as_str().as_str().cmp(right.as_str().as_str()));
            for name in lacks {
                let name = self.intern_string(name.as_str().as_str())?.raw();
                self.store.scheme_row_lacks.push(name);
            }
            self.store.scheme_row_parameters.push(FullSchemeRowParameter {
                id: parameter.id.0,
                binder: u32::try_from(binder).map_err(|_| {
                    IrBuildError::format("scheme_row_binder_overflow", None, 0, 0)
                })?,
                lacks: super::table_range(lacks_start, self.store.scheme_row_lacks.len())?,
            });
        }
        let row_parameters_range = super::table_range(
            row_parameters_start,
            self.store.scheme_row_parameters.len(),
        )?;

        let params_start = self.store.scheme_params.len();
        for parameter in scheme.params() {
            let ty = self.intern_scheme_type(&parameter.ty, &type_parameters, &row_parameters)?;
            let schema = parameter
                .schema_expectation
                .as_ref()
                .map(|expectation| self.intern_scheme_schema(expectation))
                .transpose()?;
            self.store
                .scheme_params
                .push(FullSchemeParam { ty, schema });
        }
        let params = super::table_range(params_start, self.store.scheme_params.len())?;
        let closed_return = scheme.return_type().closed_ground_type().map(SchemeType::Ground);
        let return_type = self.intern_scheme_type(
            closed_return.as_ref().unwrap_or_else(|| scheme.return_type()),
            &type_parameters,
            &row_parameters,
        )?;
        let return_schema = scheme
            .return_schema()
            .map(|expectation| self.intern_scheme_schema(expectation))
            .transpose()?;
        let effects_start = self.store.scheme_effects.len();
        if let Some(effects) = scheme.effects() {
            self.store.scheme_effects.extend_from_slice(effects);
        }
        let effects = super::table_range(effects_start, self.store.scheme_effects.len())?;
        let callable_kind = match scheme.kind() {
            SchemeCallableKind::Pure => FullSchemeCallableKind::Pure,
            SchemeCallableKind::Proc => FullSchemeCallableKind::Proc,
            SchemeCallableKind::Stream => FullSchemeCallableKind::Stream,
        };
        let id = FullSchemeId::new(self.store.schemes.len())?;
        self.store.schemes.push(FullScheme {
            type_parameters: type_parameters_range,
            row_parameters: row_parameters_range,
            params,
            return_type,
            return_schema,
            effects,
            body_facts: IrRange::default(),
            callable_kind,
            has_effects: scheme.effects().is_some(),
            explicit_return: scheme.explicit_return(),
            inferred_effects: scheme.inferred_effects(),
            reserved: [0; 3],
        });
        Ok(id)
    }

    pub(in crate::runtime::eval) fn record_call_instantiation(
        &mut self,
        instruction: usize,
        instantiation: &CallInstantiation,
    ) -> Result<(), IrBuildError> {
        if self
            .store
            .call_instantiations
            .last()
            .is_some_and(|previous| previous.instruction as usize >= instruction)
        {
            return Err(IrBuildError::format(
                "scheme_call_instantiation_order",
                None,
                0,
                instruction,
            ));
        }
        let type_parameters = self
            .active_scheme
            .map(|scheme| self.scheme_type_parameter_map(scheme))
            .transpose()?
            .unwrap_or_default();
        let row_parameters = self
            .active_scheme
            .map(|scheme| self.scheme_row_parameter_map(scheme))
            .transpose()?
            .unwrap_or_default();
        let type_arguments_start = self.store.call_type_arguments.len();
        for (binder, ty) in instantiation.type_arguments() {
            let ty = self.intern_scheme_type(ty, &type_parameters, &row_parameters)?;
            self.store.call_type_arguments.push(FullTypeArgument {
                binder: binder.0,
                ty,
            });
        }
        let type_arguments = super::table_range(
            type_arguments_start,
            self.store.call_type_arguments.len(),
        )?;
        let row_arguments_start = self.store.call_row_arguments.len();
        for (binder, evidence) in instantiation.row_arguments() {
            let fields = self.intern_row_evidence(evidence, &type_parameters, &row_parameters)?;
            let tail = match evidence.tail() {
                SchemeRowTail::Closed => IR_NONE,
                SchemeRowTail::Parameter(id) => *row_parameters.get(&id).ok_or_else(|| {
                    IrBuildError::format(
                        "scheme_call_row_tail_out_of_scope",
                        None,
                        0,
                        instruction,
                    )
                })?,
            };
            self.store.call_row_arguments.push(FullRowArgument {
                binder: binder.0,
                fields,
                tail,
            });
        }
        let row_arguments = super::table_range(
            row_arguments_start,
            self.store.call_row_arguments.len(),
        )?;
        self.store.call_instantiations.push(FullCallInstantiation {
            instruction: u32::try_from(instruction)
                .map_err(|_| IrBuildError::format("scheme_call_instruction", None, 0, instruction))?,
            type_arguments,
            row_arguments,
        });
        Ok(())
    }

    pub(super) fn record_scheme_body_fact(
        &mut self,
        function: IrFunctionId,
        scheme: FullSchemeId,
        instruction: usize,
        ty: &SchemeType,
        kind: FullSchemeBodyFactKind,
    ) -> Result<(), IrBuildError> {
        let type_parameters = self.scheme_type_parameter_map(scheme)?;
        let row_parameters = self.scheme_row_parameter_map(scheme)?;
        let ty = self.intern_scheme_type(ty, &type_parameters, &row_parameters)?;
        self.store.scheme_body_facts.push(FullSchemeBodyFact {
            instruction: u32::try_from(instruction)
                .map_err(|_| IrBuildError::format("scheme_body_instruction", None, 0, instruction))?,
            function,
            ty,
            kind,
            reserved: [0; 3],
        });
        Ok(())
    }

    pub(super) fn finish_scheme_body_facts(
        &mut self,
        scheme: FullSchemeId,
        facts_start: usize,
    ) -> Result<(), IrBuildError> {
        let facts = self
            .store
            .scheme_body_facts
            .get_mut(facts_start..)
            .ok_or_else(|| IrBuildError::format("scheme_body_fact_range", None, 0, facts_start))?;
        facts.sort_by_key(|fact| (fact.instruction, fact.kind as u8));
        let range = super::table_range(facts_start, self.store.scheme_body_facts.len())?;
        let stored = self
            .store
            .schemes
            .get_mut(scheme.index())
            .ok_or_else(|| IrBuildError::format("scheme_id", None, 0, scheme.index()))?;
        if stored.body_facts != IrRange::default() {
            return Err(IrBuildError::format(
                "scheme_body_facts_duplicate",
                None,
                0,
                scheme.index(),
            ));
        }
        stored.body_facts = range;
        Ok(())
    }

    pub(in crate::runtime::eval) fn record_checked_call_result(
        &mut self,
        instruction: usize,
        ty: &Type,
    ) -> Result<(), IrBuildError> {
        let type_id = self.intern_scheme_ground_type(ty)?;
        let instruction = u32::try_from(instruction).map_err(|_| {
            IrBuildError::format("scheme_checked_call_instruction", None, 0, instruction)
        })?;
        if let Some(previous) = self.store.checked_expr_types.last() {
            if previous.instruction == instruction {
                if previous.type_id != type_id {
                    return Err(IrBuildError::format(
                        "scheme_checked_call_result_conflict",
                        None,
                        0,
                        instruction as usize,
                    ));
                }
                return Ok(());
            }
            if previous.instruction > instruction {
                return Err(IrBuildError::format(
                    "scheme_checked_call_result_order",
                    None,
                    0,
                    instruction as usize,
                ));
            }
        }
        self.store.checked_expr_types.push(super::FullCheckedExprType {
            instruction,
            type_id,
            purpose: super::FullCheckedExprPurpose::CheckedContract { function_ref: None, with_error_source: false },
        });
        Ok(())
    }

    fn intern_scheme_type(
        &mut self,
        ty: &SchemeType,
        type_parameters: &BTreeMap<SchemeTypeParameterId, u32>,
        row_parameters: &BTreeMap<SchemeRowParameterId, u32>,
    ) -> Result<SchemeTypeId, IrBuildError> {
        let node = match ty {
            SchemeType::Ground(ty) => FullSchemeTypeNode {
                first: self.intern_scheme_ground_type(ty)?.raw(),
                second: 0,
                children: IrRange::default(),
                tag: FullSchemeTypeTag::Ground,
                reserved: [0; 3],
            },
            SchemeType::Parameter(parameter) => FullSchemeTypeNode {
                first: *type_parameters.get(parameter).ok_or_else(|| {
                    IrBuildError::format("scheme_type_parameter_scope", None, 0, 0)
                })?,
                second: 0,
                children: IrRange::default(),
                tag: FullSchemeTypeTag::Parameter,
                reserved: [0; 3],
            },
            SchemeType::List(inner) => self.unary_scheme_node(
                inner,
                type_parameters,
                row_parameters,
                FullSchemeTypeTag::List,
            )?,
            SchemeType::Stream(inner) => self.unary_scheme_node(
                inner,
                type_parameters,
                row_parameters,
                FullSchemeTypeTag::Stream,
            )?,
            SchemeType::Set(inner) => self.unary_scheme_node(
                inner,
                type_parameters,
                row_parameters,
                FullSchemeTypeTag::Set,
            )?,
            SchemeType::Optional(inner) => self.unary_scheme_node(
                inner,
                type_parameters,
                row_parameters,
                FullSchemeTypeTag::Optional,
            )?,
            SchemeType::Map(key, value) => self.binary_scheme_node(
                key,
                value,
                type_parameters,
                row_parameters,
                FullSchemeTypeTag::Map,
            )?,
            SchemeType::Result(ok, error) => self.binary_scheme_node(
                ok,
                error,
                type_parameters,
                row_parameters,
                FullSchemeTypeTag::Result,
            )?,
            SchemeType::Record { fields, tail } => {
                let mut encoded_fields = Vec::with_capacity(fields.len());
                for (name, field_type) in fields {
                    let ty = self.intern_scheme_type(
                        field_type,
                        type_parameters,
                        row_parameters,
                    )?;
                    encoded_fields.push(FullSchemeField {
                        name: self.intern_string(name.as_str().as_str())?.raw(),
                        ty,
                    });
                }
                let fields_start = self.store.scheme_fields.len();
                self.store.scheme_fields.extend(encoded_fields);
                let tail = match tail {
                    SchemeRowTail::Closed => IR_NONE,
                    SchemeRowTail::Parameter(parameter) => {
                        *row_parameters.get(parameter).ok_or_else(|| {
                            IrBuildError::format("scheme_row_parameter_scope", None, 0, 0)
                        })?
                    }
                };
                FullSchemeTypeNode {
                    first: tail,
                    second: 0,
                    children: super::table_range(fields_start, self.store.scheme_fields.len())?,
                    tag: FullSchemeTypeTag::Record,
                    reserved: [0; 3],
                }
            }
            SchemeType::Union(members) => {
                let mut children = Vec::with_capacity(members.len());
                for member in members {
                    children.push(self.intern_scheme_type(
                        member,
                        type_parameters,
                        row_parameters,
                    )?);
                }
                let children_start = self.store.scheme_children.len();
                self.store.scheme_children.extend(children);
                FullSchemeTypeNode {
                    first: 0,
                    second: 0,
                    children: super::table_range(
                        children_start,
                        self.store.scheme_children.len(),
                    )?,
                    tag: FullSchemeTypeTag::Union,
                    reserved: [0; 3],
                }
            }
            SchemeType::Validated(validation, inner) => {
                let ty = self.intern_scheme_type(inner, type_parameters, row_parameters)?;
                let validation = self.intern_scheme_validation(*validation)?;
                FullSchemeTypeNode {
                    first: ty.raw(),
                    second: validation,
                    children: IrRange::default(),
                    tag: FullSchemeTypeTag::Validated,
                    reserved: [0; 3],
                }
            }
        };
        let id = SchemeTypeId::new(self.store.scheme_types.len())?;
        self.store.scheme_types.push(node);
        Ok(id)
    }

    fn unary_scheme_node(
        &mut self,
        inner: &SchemeType,
        type_parameters: &BTreeMap<SchemeTypeParameterId, u32>,
        row_parameters: &BTreeMap<SchemeRowParameterId, u32>,
        tag: FullSchemeTypeTag,
    ) -> Result<FullSchemeTypeNode, IrBuildError> {
        let inner = self.intern_scheme_type(inner, type_parameters, row_parameters)?;
        Ok(FullSchemeTypeNode {
            first: inner.raw(),
            second: 0,
            children: IrRange::default(),
            tag,
            reserved: [0; 3],
        })
    }

    fn binary_scheme_node(
        &mut self,
        first: &SchemeType,
        second: &SchemeType,
        type_parameters: &BTreeMap<SchemeTypeParameterId, u32>,
        row_parameters: &BTreeMap<SchemeRowParameterId, u32>,
        tag: FullSchemeTypeTag,
    ) -> Result<FullSchemeTypeNode, IrBuildError> {
        let first = self.intern_scheme_type(first, type_parameters, row_parameters)?;
        let second = self.intern_scheme_type(second, type_parameters, row_parameters)?;
        Ok(FullSchemeTypeNode {
            first: first.raw(),
            second: second.raw(),
            children: IrRange::default(),
            tag,
            reserved: [0; 3],
        })
    }

    fn intern_scheme_ground_type(&mut self, ty: &Type) -> Result<TypeId, IrBuildError> {
        if ty.contains_inference() || ty.contains_recovery() {
            return Err(IrBuildError::format(
                "scheme_ground_type_not_closed",
                None,
                0,
                self.store.tags.len(),
            ));
        }
        self.semantic
            .intern_type(&mut self.store.semantic, &super::executable_type(ty))
    }

    fn intern_scheme_validation(&mut self, validation: Validation) -> Result<u32, IrBuildError> {
        let row = match validation {
            Validation::NonEmpty => FullSchemeValidation {
                name: IR_NONE,
                low: 0,
                high: 0,
                tag: FullSchemeValidationTag::NonEmpty,
                reserved: [0; 7],
            },
            Validation::RelPath => FullSchemeValidation {
                name: IR_NONE,
                low: 0,
                high: 0,
                tag: FullSchemeValidationTag::RelPath,
                reserved: [0; 7],
            },
            Validation::Nominal(name) => FullSchemeValidation {
                name: self.intern_string(name.as_str().as_str())?.raw(),
                low: 0,
                high: 0,
                tag: FullSchemeValidationTag::Nominal,
                reserved: [0; 7],
            },
            Validation::Range(range) => FullSchemeValidation {
                name: IR_NONE,
                low: range.low(),
                high: range.high(),
                tag: FullSchemeValidationTag::Range,
                reserved: [0; 7],
            },
        };
        let index = u32::try_from(self.store.scheme_validations.len())
            .map_err(|_| IrBuildError::format("scheme_validation_overflow", None, 0, 0))?;
        if index == IR_NONE {
            return Err(IrBuildError::format("scheme_validation_overflow", None, 0, 0));
        }
        self.store.scheme_validations.push(row);
        Ok(index)
    }

    fn intern_scheme_schema(
        &mut self,
        expectation: &SchemaExpectation,
    ) -> Result<FullSchemeSchemaExpectationId, IrBuildError> {
        let id = FullSchemeSchemaExpectationId::new(
            self.store.scheme_schema_expectations.len(),
        )?;
        self.store
            .scheme_schema_expectations
            .push(FullSchemeSchemaExpectation {
                instances: IrRange::default(),
                children: IrRange::default(),
            });
        let instances_start = self.store.scheme_schema_instances.len();
        for instance in &expectation.instances {
            let arguments_start = self.store.scheme_schema_arguments.len();
            for ty in &instance.arguments {
                let ty = self.intern_scheme_ground_type(ty)?;
                self.store.scheme_schema_arguments.push(ty);
            }
            self.store.scheme_schema_instances.push(FullSchemeSchemaInstance {
                definition: u32::try_from(instance.definition.index()).map_err(|_| {
                    IrBuildError::format("scheme_schema_definition", None, 0, 0)
                })?,
                arguments: super::table_range(
                    arguments_start,
                    self.store.scheme_schema_arguments.len(),
                )?,
            });
        }
        let instances = super::table_range(
            instances_start,
            self.store.scheme_schema_instances.len(),
        )?;
        let mut children = Vec::with_capacity(expectation.children.len());
        for (component, child) in expectation.children.iter() {
            let (component, name) = match component {
                SchemaComponent::Field(name) => (
                    FullSchemeSchemaComponentTag::Field,
                    self.intern_string(name.as_str().as_str())?.raw(),
                ),
                SchemaComponent::Item => (FullSchemeSchemaComponentTag::Item, IR_NONE),
                SchemaComponent::Value => (FullSchemeSchemaComponentTag::Value, IR_NONE),
                SchemaComponent::Key => (FullSchemeSchemaComponentTag::Key, IR_NONE),
                SchemaComponent::Optional => (FullSchemeSchemaComponentTag::Optional, IR_NONE),
                SchemaComponent::Success => (FullSchemeSchemaComponentTag::Success, IR_NONE),
                SchemaComponent::Error => (FullSchemeSchemaComponentTag::Error, IR_NONE),
            };
            children.push(FullSchemeSchemaChild {
                component,
                reserved: [0; 3],
                name,
                expectation: self.intern_scheme_schema(child)?,
            });
        }
        let children_start = self.store.scheme_schema_children.len();
        self.store.scheme_schema_children.extend(children);
        self.store.scheme_schema_expectations[id.index()] = FullSchemeSchemaExpectation {
            instances,
            children: super::table_range(
                children_start,
                self.store.scheme_schema_children.len(),
            )?,
        };
        Ok(id)
    }

    fn scheme_type_parameter_map(
        &self,
        scheme: FullSchemeId,
    ) -> Result<BTreeMap<SchemeTypeParameterId, u32>, IrBuildError> {
        let record = self
            .store
            .schemes
            .get(scheme.index())
            .ok_or_else(|| IrBuildError::format("scheme_id", None, 0, 0))?;
        let rows = record
            .type_parameters
            .bounds(self.store.scheme_type_parameters.len())
            .ok_or_else(|| IrBuildError::format("scheme_type_parameters", None, 0, 0))?;
        Ok(self.store.scheme_type_parameters[rows]
            .iter()
            .map(|parameter| (SchemeTypeParameterId(parameter.id), parameter.binder))
            .collect())
    }

    fn scheme_row_parameter_map(
        &self,
        scheme: FullSchemeId,
    ) -> Result<BTreeMap<SchemeRowParameterId, u32>, IrBuildError> {
        let record = self
            .store
            .schemes
            .get(scheme.index())
            .ok_or_else(|| IrBuildError::format("scheme_id", None, 0, 0))?;
        let rows = record
            .row_parameters
            .bounds(self.store.scheme_row_parameters.len())
            .ok_or_else(|| IrBuildError::format("scheme_row_parameters", None, 0, 0))?;
        Ok(self.store.scheme_row_parameters[rows]
            .iter()
            .map(|parameter| (SchemeRowParameterId(parameter.id), parameter.binder))
            .collect())
    }

    fn intern_row_evidence(
        &mut self,
        evidence: &RowEvidence,
        type_parameters: &BTreeMap<SchemeTypeParameterId, u32>,
        row_parameters: &BTreeMap<SchemeRowParameterId, u32>,
    ) -> Result<IrRange, IrBuildError> {
        let start = self.store.call_row_fields.len();
        let mut fields = evidence.fields().iter().collect::<Vec<_>>();
        fields.sort_by(|(left, _), (right, _)| {
            left.as_str().as_str().cmp(right.as_str().as_str())
        });
        for (name, ty) in fields {
            let name = self.intern_string(name.as_str().as_str())?.raw();
            let ty = self.intern_scheme_type(ty, type_parameters, row_parameters)?;
            self.store.call_row_fields.push(FullRowEvidenceField {
                name,
                ty,
            });
        }
        super::table_range(start, self.store.call_row_fields.len())
    }
}

#[cfg(test)]
mod tests {
    use super::super::FullCodec;
    use super::{FullSchemeBodyFactKind, FullSchemeTypeTag, SchemeTypeId};
    use crate::sema::schemes::{
        CallInstantiation, RowEvidence, SchemeCallableKind, SchemeParameter, SchemeRowParameter,
        SchemeRowParameterId, SchemeRowTail, SchemeType, SchemeTypeParameterId, TypeScheme,
    };
    use crate::sema::types::Type;
    use crate::symbol::Name;
    use crate::syntax::node::Effect;
    use std::collections::{BTreeMap, BTreeSet};
    use std::{cell::RefCell, rc::Rc};

    #[test]
    fn factless_pattern_id_encodes_without_an_instruction_tag() {
        let scratch = Rc::new(RefCell::new(super::super::BuildScratch::default()));
        let pattern = scratch
            .borrow_mut()
            .pattern(super::super::BuildPatternRow::Wildcard);
        let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
        builder.active_scratch = Some(scratch);
        let mut output = Vec::new();

        pattern.encode(&mut builder, &mut output).unwrap();

        assert_eq!(output, [0]);
        assert!(builder.store.tags.is_empty());
        assert_eq!(builder.store.patterns, [super::super::FullPatternTag::Wildcard]);
        assert_eq!(builder.store.pattern_data.len(), 1);
    }

    #[test]
    fn build_instruction_resolution_skips_empty_rows_and_keeps_forwarded_ids() {
        assert_eq!(
            super::super::emitted_build_instruction(&[], 0, 0, false).unwrap(),
            None
        );
        assert!(super::super::emitted_build_instruction(&[], 0, 0, true).is_err());
        assert_eq!(
            super::super::emitted_build_instruction(&[3, 1], 1, 2, true).unwrap(),
            Some(1)
        );
    }

    #[test]
    fn generic_binders_encode_outside_the_ground_type_pool() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let binder = SchemeTypeParameterId(7);
            let scheme = TypeScheme::new(
                SchemeCallableKind::Pure,
                vec![binder],
                Vec::new(),
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Parameter(binder),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(binder),
                None,
                false,
                Some(vec![Effect::Error]),
                false,
            )
            .unwrap();
            let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
            let scheme_id = builder.intern_scheme(&scheme).unwrap();
            let encoded = builder.store.scheme(scheme_id).unwrap();
            let params = encoded
                .params
                .bounds(builder.store.scheme_params.len())
                .unwrap();
            let parameter = builder.store.scheme_params[params.start];
            let param_node = builder
                .store
                .scheme_type(parameter.ty)
                .unwrap();
            let return_node = builder.store.scheme_type(encoded.return_type).unwrap();
            assert_eq!(param_node.tag, FullSchemeTypeTag::Parameter);
            assert_eq!(param_node.first, 0);
            assert_eq!(return_node.tag, FullSchemeTypeTag::Parameter);
            assert_eq!(return_node.first, 0);
            assert_eq!(builder.store.semantic.type_count(), 0);
            assert_eq!(encoded.type_parameters.len, 1);
            assert_eq!(SchemeTypeId::new(0).unwrap().index(), 0);
        });
    }

    #[test]
    fn explicit_any_is_kept_in_ground_scheme_contracts_but_not_substitutions() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let scheme = TypeScheme::new(
                SchemeCallableKind::Pure,
                Vec::new(),
                Vec::new(),
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Ground(Type::Any),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Ground(Type::Any),
                None,
                false,
                None,
                false,
            )
            .unwrap();
            let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
            let scheme_id = builder.intern_scheme(&scheme).unwrap();
            let encoded = builder.store.scheme(scheme_id).unwrap();
            let params = encoded
                .params
                .bounds(builder.store.scheme_params.len())
                .unwrap();
            let parameter = builder.store.scheme_params[params.start];
            let parameter_node = builder.store.scheme_type(parameter.ty).unwrap();
            let return_node = builder.store.scheme_type(encoded.return_type).unwrap();
            assert_eq!(parameter_node.tag, FullSchemeTypeTag::Ground);
            assert_eq!(return_node.tag, FullSchemeTypeTag::Ground);
            assert_eq!(
                builder
                    .store
                    .semantic
                    .to_type(super::TypeId::from_raw(parameter_node.first).unwrap())
                    .unwrap(),
                Type::Any
            );
            assert_eq!(
                builder
                    .store
                    .semantic
                    .to_type(super::TypeId::from_raw(return_node.first).unwrap())
                    .unwrap(),
                Type::Any
            );
        });
    }

    #[test]
    fn row_tail_lacks_and_call_substitutions_use_local_binders() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let type_parameter = SchemeTypeParameterId(7);
            let row_parameter = SchemeRowParameterId(19);
            let mut lacks = BTreeSet::new();
            lacks.insert(Name::intern("required"));
            let scheme = TypeScheme::new(
                SchemeCallableKind::Pure,
                vec![type_parameter],
                vec![SchemeRowParameter {
                    id: row_parameter,
                    lacks,
                }],
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Record {
                        fields: BTreeMap::from([(
                            Name::intern("required"),
                            SchemeType::Parameter(type_parameter),
                        )]),
                        tail: SchemeRowTail::Parameter(row_parameter),
                    },
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(type_parameter),
                None,
                false,
                None,
                false,
            )
            .unwrap();
            let evidence = RowEvidence::new(BTreeMap::from([(
                Name::intern("extra"),
                SchemeType::Ground(Type::Str),
            )]), SchemeRowTail::Closed)
            .unwrap();
            let instantiation = CallInstantiation::new(
                scheme.callable(),
                BTreeMap::from([(type_parameter, SchemeType::Ground(Type::Bool))]),
                BTreeMap::from([(row_parameter, evidence)]),
                &[],
                &[],
            )
            .unwrap();
            let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
            let scheme_id = builder.intern_scheme(&scheme).unwrap();
            builder.record_call_instantiation(4, &instantiation).unwrap();
            let encoded = builder.store.scheme(scheme_id).unwrap();
            let type_params = encoded
                .type_parameters
                .bounds(builder.store.scheme_type_parameters.len())
                .unwrap();
            let row_params = encoded
                .row_parameters
                .bounds(builder.store.scheme_row_parameters.len())
                .unwrap();
            assert_eq!(builder.store.scheme_type_parameters[type_params.start].id, 7);
            assert_eq!(builder.store.scheme_type_parameters[type_params.start].binder, 0);
            assert_eq!(builder.store.scheme_row_parameters[row_params.start].id, 19);
            assert_eq!(builder.store.scheme_row_parameters[row_params.start].binder, 0);
            let params = encoded
                .params
                .bounds(builder.store.scheme_params.len())
                .unwrap();
            let param_type = builder.store.scheme_type(
                builder.store.scheme_params[params.start].ty,
            )
            .unwrap();
            assert_eq!(param_type.tag, FullSchemeTypeTag::Record);
            assert_eq!(param_type.first, 0);
            let fields = param_type
                .children
                .bounds(builder.store.scheme_fields.len())
                .unwrap();
            assert_eq!(
                builder.store.string(builder.store.scheme_fields[fields.start].name).unwrap(),
                "required"
            );
            let lacks = builder.store.scheme_row_parameters[row_params.start]
                .lacks
                .bounds(builder.store.scheme_row_lacks.len())
                .unwrap();
            assert_eq!(
                builder.store.string(builder.store.scheme_row_lacks[lacks.start]).unwrap(),
                "required"
            );
            let call = builder.store.call_instantiations[0];
            assert_eq!(call.instruction, 4);
            let type_args = call
                .type_arguments
                .bounds(builder.store.call_type_arguments.len())
                .unwrap();
            assert_eq!(builder.store.call_type_arguments[type_args.start].binder, 7);
            assert_eq!(
                builder
                    .store
                    .semantic
                    .to_type(super::super::TypeId::from_raw(
                        builder.store.scheme_type(
                            builder.store.call_type_arguments[type_args.start].ty,
                        ).unwrap().first,
                    ).unwrap())
                    .unwrap(),
                Type::Bool
            );
            let row_args = call
                .row_arguments
                .bounds(builder.store.call_row_arguments.len())
                .unwrap();
            let row_fields = builder.store.call_row_arguments[row_args.start]
                .fields
                .bounds(builder.store.call_row_fields.len())
                .unwrap();
            assert_eq!(builder.store.call_row_arguments[row_args.start].binder, 19);
            assert_eq!(
                builder
                    .store
                    .string(builder.store.call_row_fields[row_fields.start].name)
                    .unwrap(),
                "extra"
            );
        });
    }

    #[test]
    fn body_facts_keep_final_instruction_and_checker_result_identity() {
        crate::symbol::SymbolOwner::new().with_current(|| {
            let binder = SchemeTypeParameterId(3);
            let scheme = TypeScheme::new(
                SchemeCallableKind::Pure,
                vec![binder],
                Vec::new(),
                vec![SchemeParameter {
                    name: Name::intern("value"),
                    ty: SchemeType::Parameter(binder),
                    schema_expectation: None,
                    defaulted: false,
                    rest: false,
                }],
                SchemeType::Parameter(binder),
                None,
                false,
                None,
                false,
            )
            .unwrap();
            let mut builder = super::super::FullBuilder::new(crate::source::SourceId::new(0));
            let scheme_id = builder.intern_scheme(&scheme).unwrap();
            let function = super::super::IrFunctionId::new(0).unwrap();
            let scheme_type = SchemeType::Parameter(binder);
            builder
                .record_scheme_body_fact(
                    function,
                    scheme_id,
                    17,
                    &scheme_type,
                    FullSchemeBodyFactKind::Tail,
                )
                .unwrap();
            builder
                .record_checked_call_result(21, &Type::Bool)
                .unwrap();
            builder
                .record_checked_call_result(21, &Type::Bool)
                .unwrap();
            builder.finish_scheme_body_facts(scheme_id, 0).unwrap();
            let stored = builder.store.scheme(scheme_id).unwrap();
            let facts = stored
                .body_facts
                .bounds(builder.store.scheme_body_facts.len())
                .unwrap();
            assert_eq!(builder.store.scheme_body_facts[facts.start].instruction, 17);
            assert_eq!(builder.store.scheme_body_facts[facts.start].function, function);
            assert_eq!(
                builder.store.scheme_body_facts[facts.start].kind,
                FullSchemeBodyFactKind::Tail
            );
            assert_eq!(builder.store.checked_expr_types[0].instruction, 21);
            assert_eq!(builder.store.checked_expr_types.len(), 1);
            assert_eq!(
                builder
                    .store
                    .semantic
                    .to_type(builder.store.checked_expr_types[0].type_id)
                    .unwrap(),
                Type::Bool
            );
        });
    }
}
