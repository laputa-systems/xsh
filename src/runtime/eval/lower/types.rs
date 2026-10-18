use super::{
    Arc, ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaExprKind, ArenaProgram,
    ArenaTypeExprTag, AstArena, BindingTargetId, BuildExprId, BuildExprRow, CompactDeclOutput,
    CompactLowerConstructProbe, ExprId, LoweredReturnKind,
    LoweredType, LoweredTypeCheck, Name, SlotScope, Span, Symbol, Type, TypeExprId, api_spec,
    compact_checked_type_is_concrete, concrete_checked_fact, lowered_method_name,
};

pub(super) fn checked_type_annotation(
    declarations: &CompactDeclOutput,
    namespace: Option<Name>,
    ty: TypeExprId,
) -> Option<&Type> {
    declarations
        .bodies
        .resolved_type_expressions
        .annotations
        .get(&(namespace, ty))
}

pub(super) fn stream_item_type(ty: &Type) -> Option<&Type> {
    match ty.unvalidated() {
        Type::List(item) | Type::Stream(item) => Some(item.as_ref()),
        _ => None,
    }
}

pub(super) fn compact_type_check(
    kind: LoweredType,
    checked: &Type,
    name: Arc<str>,
) -> Option<LoweredTypeCheck> {
    // A validated type stored as a scalar has a storage kind that says
    // nothing of the validation, so the type itself is tested where an
    // unchecked value arrives.
    super::super::driver_annotation_requires_validation(kind, checked).then(|| LoweredTypeCheck {
        schema: None,
        ty: checked.clone(),
        name,
    })
}

pub(super) fn compact_type_expr_name(arena: &AstArena, ty: TypeExprId) -> Arc<str> {
    compact_type_expr_name_string(arena, ty).into()
}

fn compact_type_expr_name_string(arena: &AstArena, ty: TypeExprId) -> String {
    let index = ty.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Applied => format!(
            "{}[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize)),
            arena
                .applied_type_arguments(ty)
                .map(|argument| compact_type_expr_name_string(arena, argument))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ArenaTypeExprTag::Named => Name::from_symbol(Symbol::from_raw(data.lhs)).to_string(),
        ArenaTypeExprTag::Qualified => {
            let namespace = Name::from_symbol(Symbol::from_raw(data.lhs));
            let name = Name::from_symbol(Symbol::from_raw(data.rhs));
            format!("{namespace}.{name}")
        }
        ArenaTypeExprTag::List => format!(
            "List[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::NonEmpty => format!(
            "NonEmpty[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Set => format!(
            "Set[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Map => {
            let value =
                compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize));
            match TypeExprId::from_optional_raw(data.rhs) {
                Some(key) => format!(
                    "Map[{}, {value}]",
                    compact_type_expr_name_string(arena, key)
                ),
                None => format!("Map[{value}]"),
            }
        }
        ArenaTypeExprTag::Stream => format!(
            "Stream[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Module => format!(
            "Module[{}]",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Result => {
            let ok =
                compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize));
            if let Some(err) = TypeExprId::from_optional_raw(data.rhs) {
                format!(
                    "Result[{ok}, {}]",
                    compact_type_expr_name_string(arena, err)
                )
            } else {
                format!("Result[{ok}]")
            }
        }
        ArenaTypeExprTag::Optional => format!(
            "{}?",
            compact_type_expr_name_string(arena, TypeExprId::from_index(data.lhs as usize))
        ),
        ArenaTypeExprTag::Union => format!(
            "Union[{}]",
            arena
                .union_type_members(ty)
                .map(|member| compact_type_expr_name_string(arena, member))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ArenaTypeExprTag::Callable => {
            let callable = arena.callable_type_expr(ty);
            let params = arena
                .params(callable.params)
                .iter()
                .map(|param| match param.annotation {
                    Some(annotation) => format!(
                        "{}: {}",
                        param.name,
                        compact_type_expr_name_string(arena, annotation)
                    ),
                    None => param.name.to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let effects = callable.effects.map_or(String::new(), |effects| {
                format!(
                    " [{}]",
                    arena
                        .effects(effects)
                        .map(|effect| effect.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
            format!(
                "{}({params}){effects} -> {}",
                if callable.pure { "pure" } else { "proc" },
                compact_type_expr_name_string(arena, callable.return_ty)
            )
        }
    }
}

/// The type a `guard let` target receives from a subject of type `subject`:
/// the non-null value of an optional guard, the `Ok` payload otherwise.
pub(super) fn guard_bound_type(subject: &Type, optional: bool) -> Option<&Type> {
    match subject {
        Type::Optional(present) if optional => Some(present),
        _ if optional => None,
        subject => subject.result_ok(),
    }
}

pub(super) fn record_binding_types(
    program: &ArenaProgram,
    target: BindingTargetId,
    ty: Option<&Type>,
) -> Vec<(Name, Option<Type>)> {
    match program.arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(name) => {
            if is_discard_name(name) {
                Vec::new()
            } else {
                vec![(name, ty.cloned())]
            }
        }
        ArenaBindingTargetKind::Record { fields, .. } => program
            .arena
            .destructure_fields(fields)
            .iter()
            .flat_map(|field| {
                let field_ty = match ty {
                    Some(Type::Record(schema)) => schema.get(&field.name),
                    _ => None,
                };
                record_binding_types(program, field.target, field_ty)
            })
            .collect(),
    }
}

pub(super) fn simple_binding_target(program: &ArenaProgram, id: BindingTargetId) -> Option<Name> {
    match program.arena.binding_target(id).kind {
        ArenaBindingTargetKind::Name(name) => Some(name),
        ArenaBindingTargetKind::Record { .. } => None,
    }
}

pub(super) fn is_discard_name(name: Name) -> bool {
    name == "_"
}

/// Checked lowering keeps recovery representations and leaves error facets verifier-only.
pub(super) fn lowered_checked_type(ty: &Type) -> Option<LoweredType> {
    match ty {
        Type::Invalid => Some(LoweredType::Any),
        Type::ProcessError => Some(LoweredType::Error),
        Type::ErrorFacet(_) => None,
        _ => crate::runtime::eval::indexed::full::lowered_type_for_type(ty),
    }
}

fn lowered_method_supported_for_type(ty: &Type, name: Name, arg_count: usize) -> bool {
    match ty {
        Type::Any | Type::Unknown => lowered_method_name(&name.as_str()),
        Type::Invalid => true,
        Type::Optional(inner) => lowered_method_supported_for_type(inner, name, arg_count),
        // The checker rejects a method call on a union that has not been
        // narrowed to one member, so no checked receiver has this type. A
        // receiver that does is not guessed at from its members.
        Type::Union(_) => false,
        // The checker resolved the call against the validation's own
        // receiver when that lists the method, and against the base
        // otherwise.
        Type::Validated(validated) => {
            validated
                .validation()
                .method_receiver()
                .and_then(|receiver| api_spec().method_overloads(receiver, &name.as_str()))
                .is_some_and(|methods| {
                    methods
                        .iter()
                        .any(|method| method.sig.params.len() == arg_count)
                })
                || lowered_method_supported_for_type(validated.base(), name, arg_count)
        }
        Type::Result(ok, _) => {
            name == "context" && (arg_count == 1 || arg_count == 2)
                || lowered_method_supported_for_type(ok, name, arg_count)
        }
        Type::Int | Type::UInt => {
            (name == "float" && arg_count == 0)
                || (matches!(name.as_str().as_str(), "bit_and" | "bit_or" | "clear_bits")
                    && arg_count == 1)
        }
        Type::Float => match name.as_str().as_str() {
            "floor" | "ceil" | "round" | "sqrt" | "exp" | "ln" | "sin" | "cos" | "tan" | "abs" => {
                arg_count == 0
            }
            "format" => arg_count <= 1,
            "format_number" => (1..=2).contains(&arg_count),
            "pow" | "log" | "atan2" => arg_count == 1,
            _ => false,
        },
        Type::Str => match name.as_str().as_str() {
            "trim"
            | "lower"
            | "upper"
            | "reverse"
            | "lines"
            | "words"
            | "parse_int"
            | "parse_int_decimal"
            | "parse_uint"
            | "parse_uint_positive"
            | "parse_float"
            | "base64_decode"
            | "base32_decode"
            | "count_lines"
            | "count_words"
            | "count_chars"
            | "is_empty"
            | "byte_len" => arg_count == 0,
            "fields" | "squeeze" => arg_count <= 1,
            "split" => arg_count == 1 || arg_count == 2,
            "wrap" | "delete" | "starts_with" | "ends_with" => arg_count == 1,
            "replace" | "translate" => arg_count == 2,
            "byte_at" => arg_count == 1,
            "byte_slice" | "find" => arg_count == 1 || arg_count == 2,
            _ => false,
        },
        Type::Bytes => match name.as_str().as_str() {
            "trim" | "lines" | "count_lines" | "len" | "lower" | "base64" | "base32" | "md5"
            | "sha1" | "sha256" | "sha512" | "utf8" | "is_empty" => arg_count == 0,
            "dump" | "strings" => arg_count <= 1,
            "chunks" | "compare" | "starts_with" | "ends_with" => arg_count == 1,
            "byte_at" => arg_count == 1,
            "slice" => arg_count == 1 || arg_count == 2,
            _ => false,
        },
        Type::Digest => matches!(name.as_str().as_str(), "hex" | "base64") && arg_count == 0,
        Type::Regex => match name.as_str().as_str() {
            "matches" | "find" | "captures" => arg_count == 1,
            "replace" => arg_count == 2,
            _ => false,
        },
        Type::Status => match name.as_str().as_str() {
            "exited" | "signaled" | "exit_code" | "signal_number" | "shell_code" => arg_count == 0,
            "exited_with" => arg_count == 1,
            _ => false,
        },
        Type::Path => match name.as_str().as_str() {
            "display" | "name" | "basename" | "dirname" | "ext" | "normalize" | "parent"
            | "lines" | "bytes_lines" | "read_text" | "read_bytes" | "exists" | "executable"
            | "du" | "metadata" | "readlink" | "resolve" | "remove_dir" | "unlink"
            | "read_lines" | "components" | "bytes" => arg_count == 0,
            "ext_or" => arg_count == 1,
            "with_ext" | "strip_prefix" | "relative_to" | "touch_from" | "truncate"
            | "hardlink" | "write" | "write_atomic" | "starts_with" | "ends_with"
            | "write_lines" | "glob" | "rglob" => arg_count == 1,
            "copy" | "rename" | "mkdir" | "remove" => arg_count == 1 || arg_count == 2,
            "touch" => arg_count <= 1,
            _ => false,
        },
        Type::ErasedRecord | Type::Record(_) | Type::Module(_) | Type::DynamicModule => {
            name == "get" && arg_count == 1
                || matches!(name.as_str().as_str(), "keys" | "len") && arg_count == 0
        }
        Type::Set(_) => match name.as_str().as_str() {
            "len" | "is_empty" | "to_list" => arg_count == 0,
            "add" | "remove" => arg_count == 1,
            _ => false,
        },
        Type::List(_) => match name.as_str().as_str() {
            "collect" | "len" | "is_empty" | "to_set" => arg_count == 0,
            "push" | "extend" => arg_count == 1,
            "get" => arg_count == 1,
            "join" => arg_count <= 1,
            _ => false,
        },
        Type::Map(_, _) => match name.as_str().as_str() {
            "len" | "keys" | "values" | "is_empty" => arg_count == 0,
            "remove" => arg_count == 1,
            "get" => arg_count == 1,
            "set" | "push" => arg_count == 2,
            _ => false,
        },
        Type::ProcessHandle => name == "cancel" && arg_count <= 2,
        Type::FsRoot => api_spec()
            .method_overloads(crate::modules::MethodReceiver::FsRoot, &name.as_str())
            .is_some_and(|methods| {
                methods.iter().any(|method| {
                    arg_count <= method.sig.params.len()
                        && arg_count
                            >= method
                                .sig
                                .params
                                .iter()
                                .filter(|param| !param.defaulted)
                                .count()
                })
            }),
        Type::NetJob => matches!(name.as_str().as_str(), "wait" | "cancel") && arg_count == 0,
        Type::Stream(_) => name == "collect" && arg_count == 0,
        _ => false,
    }
}

pub(super) fn checked_fact_is_resolved(ty: &Type) -> bool {
    !ty.contains_inference()
        && match ty {
            Type::Unknown | Type::Invalid => false,
            Type::List(inner) | Type::Stream(inner) | Type::Optional(inner) | Type::Set(inner) => {
                checked_fact_is_resolved(inner)
            }
            Type::Map(key, value) | Type::Result(key, value) => {
                checked_fact_is_resolved(key) && checked_fact_is_resolved(value)
            }
            Type::Record(fields) => fields.values().all(checked_fact_is_resolved),
            Type::Validated(validated) => checked_fact_is_resolved(validated.base()),
            _ => true,
        }
}

pub(super) fn type_for_lowered_type(kind: LoweredType) -> Option<Type> {
    match kind {
        // Set elements and tag identities are not part of the kind.
        LoweredType::Set | LoweredType::Tag => None,
        kind => Some(crate::runtime::eval::indexed::full::lowered_type_to_type(kind)),
    }
}

impl<'p> CompactLowerConstructProbe<'p, '_> {
    /// The checker's published type for `value` when it is concrete, including
    /// a concrete `Result` success type.
    pub(super) fn concrete_checked_type(&self, value: ExprId) -> Option<Type> {
        self.bodies
            .expr_types
            .get(&value)
            .filter(|ty| concrete_checked_fact(ty))
            .cloned()
    }

    /// Runtime checks use declaration identities, so resolve import aliases in
    /// checked annotations before comparing them with lowered values.
    pub(super) fn canonical_checked_type_annotation(&self, ty: TypeExprId) -> Option<Type> {
        checked_type_annotation(self.declarations, self.current_namespace, ty)
            .cloned()
            .map(|ty| self.canonicalize_checked_type(ty))
    }

    pub(super) fn lower_binding_checked_type(&self, ty: Option<TypeExprId>, value: ExprId) -> Option<Type> {
        match ty {
            Some(ty) => self.canonical_checked_type_annotation(ty),
            None => {
                self.bodies
                    .expr_types
                    .get(&value)
                    .filter(|ty| !matches!(ty, Type::Invalid))
                    .cloned()
            }
        }
    }

    pub(super) fn checked_unsigned_value(&mut self, value: BuildExprId, ty: &Type, span: Span) -> BuildExprId {
        if !ty.has_unsigned_constraint() {
            return value;
        }
        let check = LoweredTypeCheck {
            ty: ty.clone(),
            name: Arc::from(ty.to_string()),
            schema: None,
        };
        push_build_row!(
            self,
            expr,
            BuildExprRow::CheckedValue { value, check, span }
        )
    }

    pub(super) fn require_uint_key(&mut self, value: BuildExprId, span: Span) -> BuildExprId {
        let check = LoweredTypeCheck {
            ty: Type::UInt,
            name: Arc::from("UInt"),
            schema: None,
        };
        let checked = push_build_row!(self, expr, BuildExprRow::Require { value, check, span });
        push_build_row!(
            self,
            expr,
            BuildExprRow::Try {
                value: checked,
                span
            }
        )
    }

    pub(super) fn lower_binding_expr_value(
        &mut self,
        ty: Option<TypeExprId>,
        checked_ty: Option<&Type>,
        value: ExprId,
        span: Span,
        slots: &mut SlotScope,
        current_function: Option<Name>,
        item_slot: Option<usize>,
    ) -> Option<BuildExprId> {
        let lowered = self.lower_expr(value, slots, current_function, item_slot)?;
        let kind = self
            .bodies
            .expr_types
            .get(&value)
            .map(|ty| ty.optional_inner().unwrap_or(ty))
            .and_then(lowered_checked_type)
            .unwrap_or(LoweredType::Any);
        self.lower_binding_storage_value(ty, checked_ty, Some(kind), lowered, span)
    }

    pub(super) fn lower_binding_storage_value(
        &mut self,
        ty: Option<TypeExprId>,
        checked_ty: Option<&Type>,
        kind: Option<LoweredType>,
        lowered: BuildExprId,
        span: Span,
    ) -> Option<BuildExprId> {
        let Some(ty) = ty else {
            return Some(match checked_ty {
                Some(ty) => self.checked_unsigned_value(lowered, ty, span),
                None => lowered,
            });
        };
        let kind = kind?;
        let annotation = match checked_ty {
            Some(annotation) => annotation,
            None => checked_type_annotation(
                self.declarations,
                self.current_namespace,
                ty,
            )?,
        };
        if super::super::driver_annotation_requires_validation(kind, annotation) {
            let check = LoweredTypeCheck {
                schema: None,
                ty: self.canonicalize_checked_type(annotation.clone()),
                name: compact_type_expr_name(&self.program.arena, ty),
            };
            Some(push_build_row!(
                self,
                expr,
                BuildExprRow::Try {
                    value: push_build_row!(
                        self,
                        expr,
                        BuildExprRow::Require {
                            value: lowered,
                            check,
                            span,
                        }
                    ),
                    span
                }
            ))
        } else {
            Some(lowered)
        }
    }

    pub(super) fn lowered_method_supported_for_receiver(
        &self,
        base: ExprId,
        name: Name,
        arg_count: usize,
        slots: &SlotScope,
    ) -> bool {
        if let Some(binding) = self.expression_binding(base)
            && let Some(ty) = slots.binding_type(binding)
        {
            // A slot declared as a union holds one member wherever a method
            // is called on it, and which member is the checker's decision,
            // published as this receiver's type. Without that fact the call
            // is not lowered: the declared union cannot say which member's
            // methods apply.
            let declared_union = match ty {
                Type::Union(_) => true,
                Type::Optional(inner) => matches!(inner.as_ref(), Type::Union(_)),
                _ => false,
            };
            if declared_union {
                return self.checked_expr_type(base).is_some_and(|narrowed| {
                    self.lowered_method_supported_for_type(&narrowed, name, arg_count)
                });
            }
            // A slot of a base type holds a validated value where a type
            // test proved it, and the checker then resolved the call against
            // the validated type; that fact, not the declaration, says which
            // methods the receiver has.
            if let Some(narrowed) = self
                .checked_expr_type(base)
                .filter(|narrowed| narrowed.validated().is_some())
            {
                return self.lowered_method_supported_for_type(&narrowed, name, arg_count);
            }
            return self.lowered_method_supported_for_type(ty, name, arg_count);
        }
        let Some(ty) = self
            .checked_expr_type(base)
            .or_else(|| self.concrete_checked_type(base))
        else {
            // A local without a declared slot type (a loop item, a pattern
            // capture) may hold a union member, and only the checker's fact
            // says which. Without it the call is not lowered.
            return !matches!(
                self.program.arena.expr(base).kind,
                ArenaExprKind::Ident(_) if self.source_slot(base, slots).is_some()
            );
        };
        self.lowered_method_supported_for_type(&ty, name, arg_count)
    }

    fn lowered_method_supported_for_type(&self, ty: &Type, name: Name, arg_count: usize) -> bool {
        lowered_method_supported_for_type(ty, name, arg_count)
    }

    pub(super) fn loop_item_checked_type(&self, iter: ExprId) -> Option<Type> {
        self.checked_expr_type(iter)
            .or_else(|| self.concrete_checked_type(iter))
            .or_else(|| self.bodies.expr_types.get(&iter).cloned())
            .and_then(|ty| ty.iteration_item_type())
    }

    /// Extract the concrete ok and err payload types from a match scrutinee so
    /// `Ok(binding)`/`Err(binding)` arms can declare their slot with a checked
    /// type. Returns `None` for either side when the scrutinee is not a
    /// `Result` or when that payload type is not concrete, so callers keep the
    /// previous untyped-slot behavior instead of forcing `Any`.
    pub(super) fn compact_match_scrutinee_result_types(
        &self,
        scrutinee: ExprId,
    ) -> (Option<Type>, Option<Type>) {
        let scrutinee_ty = self
            .checked_expr_type(scrutinee)
            .or_else(|| self.concrete_checked_type(scrutinee));
        match scrutinee_ty {
            Some(Type::Result(ok, err)) => {
                let ok_ty = compact_checked_type_is_concrete(&ok).then(|| ok.as_ref().clone());
                let err_ty = compact_checked_type_is_concrete(&err).then(|| err.as_ref().clone());
                (ok_ty, err_ty)
            }
            _ => (None, None),
        }
    }

    /// The checker's published type for `value`.
    pub(super) fn checked_expr_type(&self, value: ExprId) -> Option<Type> {
        self.bodies
            .expr_types
            .get(&value)
            .filter(|ty| checked_fact_is_resolved(ty))
            .cloned()
    }

    pub(super) fn is_empty_record_in_map_context(&self, value: ExprId, ty: Option<TypeExprId>) -> bool {
        ty.is_some_and(|ty| self.program.arena.type_expr_tags[ty.index()] == ArenaTypeExprTag::Map)
            && matches!(self.program.arena.expr(value).kind, ArenaExprKind::Record(fields) if self.program.arena.record_fields(fields).is_empty())
    }

    pub(super) fn lowered_return_kind(&self, ty: TypeExprId) -> Option<LoweredReturnKind> {
        let ty = checked_type_annotation(self.declarations, self.current_namespace, ty)?;
        match ty.unvalidated() {
            Type::Result(ok, _) => Some(LoweredReturnKind::Result(lowered_checked_type(ok)?)),
            Type::Optional(inner) if matches!(inner.unvalidated(), Type::Result(_, _)) => {
                Some(LoweredReturnKind::OptionalResult)
            }
            ty => Some(LoweredReturnKind::Plain(lowered_checked_type(ty)?)),
        }
    }

    pub(super) fn assign_target_checked_type(
        &self,
        id: crate::syntax::arena::AssignTargetId,
        span: Span,
        slots: &SlotScope,
    ) -> Option<Type> {
        match self.program.arena.assign_target(id).kind {
            ArenaAssignTargetKind::Name(name) => slots.binding_type(self.bodies.resolved_bindings.at(span, name)?).cloned(),
            ArenaAssignTargetKind::Field { base, name } => {
                match self.assign_target_checked_type(base, span, slots)? {
                    Type::Record(fields) => fields.get(&name).cloned(),
                    _ => None,
                }
            }
            ArenaAssignTargetKind::Index { base, .. } => {
                match self
                    .assign_target_checked_type(base, span, slots)?
                    .into_unvalidated()
                {
                    Type::List(item) | Type::Map(_, item) => Some(*item),
                    _ => None,
                }
            }
            ArenaAssignTargetKind::Env(_) => None,
        }
    }

    pub(super) fn prepared_schema(&self, ty: Type) -> Arc<super::super::require::PreparedSchema> {
        if let Some(schema) = self
            .scratch
            .borrow()
            .prepared_schemas
            .iter()
            .find_map(|(prepared, schema)| (*prepared == ty).then(|| schema.clone()))
        {
            return schema;
        }
        let schema = super::super::require::PreparedSchema::compile(&ty, &self.declarations.wire_enums);
        self.scratch
            .borrow_mut()
            .prepared_schemas
            .push((ty, schema.clone()));
        schema
    }
}

#[cfg(test)]
mod mapping_tests {
    use super::*;

    #[test]
    fn checked_type_mapping_keeps_its_recovery_policy() {
        assert_eq!(lowered_checked_type(&Type::Invalid), Some(LoweredType::Any));
        assert_eq!(
            lowered_checked_type(&Type::ProcessError),
            Some(LoweredType::Error)
        );
        assert_eq!(
            lowered_checked_type(&Type::ErrorFacet(Name::intern("Error"))),
            None
        );
        assert_eq!(
            lowered_checked_type(&Type::non_empty(Type::Int)),
            Some(LoweredType::List)
        );
        assert_eq!(
            type_for_lowered_type(LoweredType::Set),
            None,
            "the kind does not retain the set element type"
        );
        assert_eq!(
            type_for_lowered_type(LoweredType::Tag),
            None,
            "the kind does not retain tag identity"
        );
        assert_eq!(
            type_for_lowered_type(LoweredType::Result),
            Some(Type::Result(Box::new(Type::Any), Box::new(Type::Error)))
        );
    }
}
