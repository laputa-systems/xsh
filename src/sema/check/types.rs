#![allow(clippy::single_call_fn)]

use super::{
    BTreeMap, CallableParamType, CallableType, Checker, ContractParam, Diagnostic, Effect, Label,
    ModuleContractEntryKind, ModuleExportType, Span, Type, TypeAnnRef, TypeDefBody,
};
use crate::sema::records::standard_record_type;
use crate::symbol::{Name, Symbol};
use crate::syntax::arena::{ArenaProgram, ArenaTypeExprTag, TypeExprId};
use xsh_registry::types::BuiltinTypeName;

impl Checker {
    pub(super) fn check_propagation(&mut self, ty: &Type, span: Span) -> Type {
        if self.retry_attempt_depth > 0 {
            return self.check_attempt_propagation(ty, span);
        }
        if matches!(ty, Type::Unknown | Type::Invalid) {
            return ty.clone();
        }
        if matches!(ty, Type::Any) {
            return Type::Any;
        }
        if let Some(effs) = &self.current_effects
            && !effs.contains(&Effect::Error)
        {
            self.error(
                span,
                "`?` requires the `error` effect",
                "check.effect-violation",
            );
        }
        let Some((ok, err)) = result_types(ty) else {
            self.error(
                span,
                "`?` can be applied only to Result values",
                "check.try-result",
            );
            return Type::Unknown;
        };
        let allowed = self
            .current_effects
            .as_ref()
            .is_some_and(|effects| effects.contains(&Effect::Error))
            || self
                .current_return
                .as_ref()
                .is_none_or(|return_ty| return_ty.is_result())
            || self.current_yield.is_some();
        if !allowed {
            self.error(
                span,
                "`?` requires a Result-returning context",
                "check.try-context",
            );
        }
        if let Some(Type::Result(_, return_err)) = &self.current_return
            && !err.matches_expected(return_err)
        {
            self.diagnostics.push(
                Diagnostic::error("incompatible propagated error")
                    .with_code("check.try-error")
                    .with_label(Label::primary(
                        span,
                        format!("cannot propagate {err} from function returning {return_err}"),
                    )),
            );
        }
        ok
    }

    pub(super) fn check_attempt_propagation(&mut self, ty: &Type, span: Span) -> Type {
        if matches!(ty, Type::Unknown | Type::Invalid) {
            return ty.clone();
        }
        if matches!(ty, Type::Any) {
            return Type::Any;
        }
        let Some((ok, _)) = result_types(ty) else {
            self.error(
                span,
                "`?` can be applied only to Result values",
                "check.try-result",
            );
            return Type::Unknown;
        };
        ok
    }

    pub(super) fn reject_ignored_result(&mut self, ty: &Type, span: Span) {
        if ty.is_result() {
            self.error(span, "ignored Result value", "check.ignored-result");
        }
    }

    pub(super) fn expect_type(&mut self, expected: &Type, actual: &Type, span: Span) {
        if self.options.strict_dynamic && actual.any_flows_to_concrete(expected) {
            self.warning(
                span,
                "strict mode requires schema check before using Any as a concrete type",
                "check.strict-any",
            );
        }
        if !actual.matches_expected(expected) {
            self.diagnostics.push(
                Diagnostic::error("type mismatch")
                    .with_code("check.type-mismatch")
                    .with_label(Label::primary(
                        span,
                        format!("expected {expected}, found {actual}"),
                    )),
            );
        }
    }

    pub(super) fn type_from_ann(&mut self, type_ann: &TypeAnnRef) -> Type {
        self.type_from_arena(&type_ann.program, type_ann.id)
    }

    pub(super) fn type_from_arena(&mut self, program: &ArenaProgram, type_id: TypeExprId) -> Type {
        let tag = program.arena.type_expr_tags[type_id.index()];
        let data = program.arena.type_expr_data[type_id.index()];
        let span = program.arena.type_expr_span(type_id);
        match tag {
            ArenaTypeExprTag::Named => {
                let name = Name::from_symbol(Symbol::from_raw(data.lhs));
                self.type_from_name(name, span)
            }
            ArenaTypeExprTag::Qualified => {
                let namespace = Name::from_symbol(Symbol::from_raw(data.lhs));
                let name = Name::from_symbol(Symbol::from_raw(data.rhs));
                self.type_from_qualified_name(namespace, name, span)
            }
            ArenaTypeExprTag::List => Type::List(Box::new(
                self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize)),
            )),
            ArenaTypeExprTag::Map => Type::Map(Box::new(
                self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize)),
            )),
            ArenaTypeExprTag::Stream => Type::Stream(Box::new(
                self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize)),
            )),
            ArenaTypeExprTag::Module => {
                let inner = TypeExprId::from_index(data.lhs as usize);
                match self.type_from_arena(program, inner) {
                    Type::Module(exports) => Type::Module(exports),
                    Type::Record(fields) => {
                        let exports = fields
                            .into_iter()
                            .map(|(name, ty)| {
                                (
                                    name,
                                    ModuleExportType::Value {
                                        ty,
                                        optional: false,
                                    },
                                )
                            })
                            .collect();
                        Type::Module(exports)
                    }
                    Type::Unknown | Type::Invalid => Type::Module(BTreeMap::new()),
                    other => {
                        self.error(
                            program.arena.type_expr_span(inner),
                            &format!("Module[...] expected a module contract, found `{other}`"),
                            "check.type-mismatch",
                        );
                        Type::Invalid
                    }
                }
            }
            ArenaTypeExprTag::Result => Type::Result(
                Box::new(self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize))),
                Box::new(
                    TypeExprId::from_optional_raw(data.rhs)
                        .map_or(Type::Error, |err| self.type_from_arena(program, err)),
                ),
            ),
            ArenaTypeExprTag::Optional => Type::Optional(Box::new(
                self.type_from_arena(program, TypeExprId::from_index(data.lhs as usize)),
            )),
        }
    }

    pub(super) fn type_from_name(&mut self, name: Name, span: Span) -> Type {
        if BuiltinTypeName::parse(&name.as_str()) == Some(BuiltinTypeName::Unknown) {
            self.error(
                span,
                "`Unknown` is not a source type; use `Any` for dynamic values",
                "check.unknown-type",
            );
            return Type::Invalid;
        }
        if let Some(builtin) = Type::builtin_from_name(&name.as_str()) {
            return builtin;
        }
        if let Some(record) = standard_record_type(&name.as_str()) {
            return record;
        }
        if self.error_families.contains_key(&name) {
            return Type::ErrorFamily(name);
        }
        if self.error_facets.contains(&name) {
            return Type::ErrorFacet(name);
        }
        let Some(body) = self.type_defs.get(&name).cloned() else {
            self.error(span, "unknown type", "check.unknown-type");
            return Type::Invalid;
        };
        self.type_from_body(name, body, span)
    }

    pub(super) fn type_from_qualified_name(
        &mut self,
        namespace: Name,
        name: Name,
        span: Span,
    ) -> Type {
        let qualified = Name::intern(format!("{namespace}.{name}"));
        if self.error_families.contains_key(&qualified) {
            return Type::ErrorFamily(qualified);
        }
        if self.error_facets.contains(&qualified) {
            return Type::ErrorFacet(qualified);
        }
        let Some(types) = self.type_namespaces.get(&namespace) else {
            self.error(span, "unknown type namespace", "check.unknown-type");
            return Type::Invalid;
        };
        let Some(ty) = types.get(&name).cloned() else {
            self.error(span, "unknown exported type", "check.unknown-type");
            return Type::Invalid;
        };
        ty
    }

    pub(super) fn type_from_body(&mut self, key: Name, body: TypeDefBody, span: Span) -> Type {
        if self.resolving_types.contains(&key) {
            self.error(
                span,
                "recursive type aliases are not supported",
                "check.recursive-type",
            );
            return Type::Invalid;
        }
        self.resolving_types.push(key);
        let ty = match body {
            TypeDefBody::Resolved(ty) => ty,
            TypeDefBody::Alias(alias) => self.type_from_ann(&alias),
            TypeDefBody::RecordSchema(fields) => {
                let mut record = BTreeMap::new();
                for field in fields {
                    record.insert(field.name, self.type_from_ann(&field.ty));
                }
                Type::Record(record)
            }
            TypeDefBody::ModuleContract(entries) => {
                let mut exports = BTreeMap::new();
                for entry in entries {
                    let export_ty = match entry.kind {
                        ModuleContractEntryKind::Value(ty) => ModuleExportType::Value {
                            ty: self.type_from_ann(&ty),
                            optional: entry.optional,
                        },
                        ModuleContractEntryKind::Proc {
                            params,
                            effects,
                            return_ty,
                        } => ModuleExportType::Proc {
                            sig: self.callable_type_from_parts(params, effects, return_ty),
                            optional: entry.optional,
                        },
                        ModuleContractEntryKind::Pure { params, return_ty } => {
                            ModuleExportType::Pure {
                                sig: self.callable_type_from_parts(params, None, return_ty),
                                optional: entry.optional,
                            }
                        }
                    };
                    exports.insert(entry.name, export_ty);
                }
                Type::Module(exports)
            }
            TypeDefBody::TagUnion(_) => Type::Tag(key),
        };
        self.resolving_types.pop();
        ty
    }

    fn callable_type_from_parts(
        &mut self,
        params: Vec<ContractParam>,
        effects: Option<Vec<Effect>>,
        return_ty: TypeAnnRef,
    ) -> CallableType {
        CallableType {
            params: params
                .into_iter()
                .map(|param| CallableParamType {
                    name: param.name,
                    ty: self.type_from_ann(&param.ty),
                    defaulted: param.defaulted,
                    rest: param.rest,
                })
                .collect(),
            return_ty: Box::new(self.type_from_ann(&return_ty)),
            effects,
        }
    }
}

pub(super) fn tail_type_matches_expected(expected: &Type, actual: &Type) -> bool {
    actual.matches_expected(expected)
        || (!actual.is_result()
            && matches!(expected, Type::Result(ok, _) if actual.matches_expected(ok)))
}

pub(super) fn result_types(ty: &Type) -> Option<(Type, Type)> {
    match ty {
        Type::Result(ok, err) => Some((ok.as_ref().clone(), err.as_ref().clone())),
        Type::Unknown => Some((Type::Unknown, Type::Unknown)),
        Type::Any => Some((Type::Any, Type::Any)),
        _ => None,
    }
}

pub(super) fn collection_item_ty(ty: &Type) -> Type {
    match ty {
        Type::List(item) => item.as_ref().clone(),
        Type::Unknown => Type::Unknown,
        Type::Any => Type::Any,
        _ => Type::Unknown,
    }
}

pub(super) fn map_item_ty(ty: &Type) -> Type {
    match ty {
        Type::Map(item) => item.as_ref().clone(),
        Type::Unknown => Type::Unknown,
        Type::Any => Type::Any,
        _ => Type::Unknown,
    }
}

pub(super) fn merge_collection_item_ty(primary: Type, fallback: Type) -> Type {
    if primary == Type::Unknown {
        fallback
    } else if primary == Type::Any
        || fallback == Type::Any
        || matches!(
            (&primary, &fallback),
            (Type::Str, Type::Path) | (Type::Path, Type::Str)
        )
    {
        Type::Any
    } else {
        primary
    }
}
