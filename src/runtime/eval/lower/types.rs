use super::{
    Arc, ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaExprKind, ArenaProgram,
    ArenaTypeExprTag, AstArena, BindingTargetId, BuildExprId, BuildExprRow, BuiltinTypeName,
    CompactDeclOutput, CompactLowerConstructProbe, CompactTypeDefInfo, ExprId, LoweredReturnKind,
    LoweredType, LoweredTypeCheck, Name, SlotScope, Span, Symbol, Type, TypeExprId, api_spec,
    compact_checked_type_is_concrete, concrete_checked_fact, lowered_arena_type,
    lowered_method_name, standard_record_type,
};

/// The bounded type a declaration names, over the base its alias resolves
/// to. The checker accepted the declaration, so the base fits; a base that
/// does not is left unbounded rather than given bounds it cannot have.
fn compact_bounded_type(range: crate::sema::validated::IntRange, base: Type) -> Type {
    Type::bounded(range, base.clone()).unwrap_or(base)
}

pub(super) fn lowered_arena_type_inner(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
    depth: usize,
) -> Option<LoweredType> {
    if depth > declarations.types.len() {
        return None;
    }
    let index = ty.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Applied => lowered_checked_type(
            &declarations
                .record_constructors
                .resolve_type(arena, ty, None),
        ),
        ArenaTypeExprTag::Named => {
            let name = Name::from_symbol(crate::symbol::Symbol::from_raw(data.lhs));
            if let Some(lowered) = lowered_builtin_type_name(&name.as_str()) {
                return Some(lowered);
            }
            if standard_record_type(&name.as_str()).is_some() {
                return Some(LoweredType::Record);
            }
            if declarations.error_families_by_name.contains_key(&name) {
                return Some(LoweredType::Error);
            }
            match declarations.types.get(&name) {
                // A bounded integer is stored as its base.
                Some(CompactTypeDefInfo::Alias(alias) | CompactTypeDefInfo::Bounded(alias, _)) => {
                    lowered_arena_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Record(_)) => Some(LoweredType::Record),
                Some(CompactTypeDefInfo::Module(_)) => Some(LoweredType::Module),
                Some(CompactTypeDefInfo::TagUnion) => Some(LoweredType::Tag),
                None => Some(LoweredType::Record),
            }
        }
        ArenaTypeExprTag::Qualified => {
            let name = Name::from_symbol(crate::symbol::Symbol::from_raw(data.rhs));
            match declarations.types.get(&name) {
                // A bounded integer is stored as its base.
                Some(CompactTypeDefInfo::Alias(alias) | CompactTypeDefInfo::Bounded(alias, _)) => {
                    lowered_arena_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Record(_)) => Some(LoweredType::Record),
                Some(CompactTypeDefInfo::Module(_)) => Some(LoweredType::Module),
                Some(CompactTypeDefInfo::TagUnion) => Some(LoweredType::Tag),
                None => Some(LoweredType::Record),
            }
        }
        // A validated type is stored as its base.
        ArenaTypeExprTag::List | ArenaTypeExprTag::NonEmpty => Some(LoweredType::List),
        ArenaTypeExprTag::Map => Some(LoweredType::Map),
        ArenaTypeExprTag::Stream => Some(LoweredType::Stream),
        ArenaTypeExprTag::Set => Some(LoweredType::Set),
        ArenaTypeExprTag::Module => Some(LoweredType::Module),
        ArenaTypeExprTag::Result => Some(LoweredType::Result),
        // A union has no single runtime representation; its members keep
        // their own.
        ArenaTypeExprTag::Optional | ArenaTypeExprTag::Union => Some(LoweredType::Any),
        // A typed callable is the dynamic handle of its kind at run time.
        ArenaTypeExprTag::Callable => Some(if arena.callable_type_expr(ty).pure {
            LoweredType::Pure
        } else {
            LoweredType::Proc
        }),
    }
}

pub(super) fn stream_item_type(ty: &Type) -> Option<&Type> {
    match ty.unvalidated() {
        Type::List(item) | Type::Stream(item) => Some(item.as_ref()),
        _ => None,
    }
}

pub(super) fn compact_pattern_test_type(
    arena: &AstArena,
    name: Name,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Type {
    if declarations.error_families_by_name.contains_key(&name) {
        return Type::ErrorFamily(name);
    }
    if declarations.error_families_by_name.values().any(|family| {
        family
            .variants
            .values()
            .any(|variant| variant.facets.contains(&name))
    }) || xsh_registry::errors::ErrorFacet::from_name(&name.as_str()).is_some()
    {
        return Type::ErrorFacet(name);
    }
    compact_runtime_type(arena, ty, declarations)
}

fn compact_runtime_type(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
) -> Type {
    compact_runtime_type_in_namespace(arena, ty, declarations, None)
}

pub(super) fn compact_runtime_type_in_namespace(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
    namespace: Option<Name>,
) -> Type {
    let resolved = declarations
        .record_constructors
        .resolve_type(arena, ty, namespace);
    if !matches!(resolved, Type::Unknown | Type::Invalid) {
        return resolved;
    }
    compact_runtime_type_inner(arena, ty, declarations, 0)
}

fn compact_runtime_type_inner(
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
    depth: usize,
) -> Type {
    if depth > declarations.types.len() {
        return Type::Unknown;
    }
    let index = ty.index();
    let tag = arena.type_expr_tags[index];
    let data = arena.type_expr_data[index];
    match tag {
        ArenaTypeExprTag::Applied => Type::Invalid,
        ArenaTypeExprTag::Named => {
            let name = Name::from_symbol(Symbol::from_raw(data.lhs));
            if let Some(builtin) = BuiltinTypeName::parse(&name.as_str()) {
                return Type::from_builtin_name(builtin);
            }
            if let Some(record) = standard_record_type(&name.as_str()) {
                return record;
            }
            if declarations.error_families_by_name.contains_key(&name) {
                return Type::ErrorFamily(name);
            }
            match declarations.types.get(&name) {
                Some(CompactTypeDefInfo::Alias(alias)) => {
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Bounded(alias, range)) => compact_bounded_type(
                    *range,
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1),
                ),
                Some(CompactTypeDefInfo::Record(_)) => {
                    compact_record_type(arena, name, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Module(exports)) => Type::Module(exports.clone()),
                Some(CompactTypeDefInfo::TagUnion) => Type::Tag(name),
                None => Type::ErasedRecord,
            }
        }
        ArenaTypeExprTag::Qualified => {
            let name = Name::from_symbol(Symbol::from_raw(data.rhs));
            match declarations.types.get(&name) {
                Some(CompactTypeDefInfo::Alias(alias)) => {
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Bounded(alias, range)) => compact_bounded_type(
                    *range,
                    compact_runtime_type_inner(arena, *alias, declarations, depth + 1),
                ),
                Some(CompactTypeDefInfo::Record(_)) => {
                    compact_record_type(arena, name, declarations, depth + 1)
                }
                Some(CompactTypeDefInfo::Module(exports)) => Type::Module(exports.clone()),
                Some(CompactTypeDefInfo::TagUnion) => Type::Tag(name),
                None => Type::ErasedRecord,
            }
        }
        ArenaTypeExprTag::List => Type::List(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::NonEmpty => Type::non_empty(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        )),
        ArenaTypeExprTag::Set => Type::Set(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Map => Type::Map(
            Box::new(
                TypeExprId::from_optional_raw(data.rhs).map_or(Type::Str, |id| {
                    compact_runtime_type_inner(arena, id, declarations, depth)
                }),
            ),
            Box::new(compact_runtime_type_inner(
                arena,
                TypeExprId::from_index(data.lhs as usize),
                declarations,
                depth,
            )),
        ),
        ArenaTypeExprTag::Stream => Type::Stream(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Module => Type::DynamicModule,
        ArenaTypeExprTag::Result => Type::Result(
            Box::new(compact_runtime_type_inner(
                arena,
                TypeExprId::from_index(data.lhs as usize),
                declarations,
                depth,
            )),
            Box::new(
                TypeExprId::from_optional_raw(data.rhs).map_or(Type::Error, |err| {
                    compact_runtime_type_inner(arena, err, declarations, depth)
                }),
            ),
        ),
        ArenaTypeExprTag::Optional => Type::Optional(Box::new(compact_runtime_type_inner(
            arena,
            TypeExprId::from_index(data.lhs as usize),
            declarations,
            depth,
        ))),
        ArenaTypeExprTag::Union => Type::Union(
            arena
                .union_type_members(ty)
                .map(|member| compact_runtime_type_inner(arena, member, declarations, depth))
                .collect(),
        ),
        // The signature is a checked fact, not something a slot can test: a
        // slot of a callable type holds the dynamic handle of its kind.
        ArenaTypeExprTag::Callable => {
            if arena.callable_type_expr(ty).pure {
                Type::Pure
            } else {
                Type::Proc
            }
        }
    }
}

fn compact_record_type(
    arena: &AstArena,
    name: Name,
    declarations: &CompactDeclOutput,
    depth: usize,
) -> Type {
    if let Some(fields) = declarations.record_schema_fields.get(&name) {
        return Type::Record(
            fields
                .iter()
                .map(|(field, ty)| {
                    (
                        *field,
                        compact_runtime_type_inner(arena, *ty, declarations, depth),
                    )
                })
                .collect(),
        );
    }
    match declarations.types.get(&name) {
        Some(CompactTypeDefInfo::Record(fields)) => Type::Record(fields.clone()),
        _ => Type::Unknown,
    }
}

pub(super) fn compact_type_check(
    kind: LoweredType,
    arena: &AstArena,
    ty: TypeExprId,
    declarations: &CompactDeclOutput,
    namespace: Option<Name>,
) -> Option<LoweredTypeCheck> {
    let checked = compact_runtime_type_in_namespace(arena, ty, declarations, namespace);
    // A validated type stored as a scalar has a storage kind that says
    // nothing of the validation, so the type itself is tested where an
    // unchecked value arrives.
    (lowered_type_needs_static_check(kind)
        || checked.has_unsigned_constraint()
        || checked.validated().is_some())
    .then(|| LoweredTypeCheck {
        schema: None,
        ty: checked,
        name: compact_type_expr_name(arena, ty),
    })
}

pub(super) fn lowered_type_needs_static_check(kind: LoweredType) -> bool {
    matches!(
        kind,
        LoweredType::Error
            | LoweredType::Record
            | LoweredType::Module
            | LoweredType::List
            | LoweredType::Stream
            | LoweredType::Map
            | LoweredType::Set
            | LoweredType::Tag
            | LoweredType::Result
            | LoweredType::Any
    )
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
                .map(|param| {
                    format!(
                        "{}: {}",
                        param.name,
                        compact_type_expr_name_string(arena, param.ty)
                    )
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

pub(super) fn lowered_checked_type(ty: &Type) -> Option<LoweredType> {
    match ty {
        Type::Unit => Some(LoweredType::Unit),
        Type::Int | Type::UInt => Some(LoweredType::Int),
        Type::Float => Some(LoweredType::Float),
        Type::Duration => Some(LoweredType::Duration),
        Type::Bool => Some(LoweredType::Bool),
        Type::Str => Some(LoweredType::Str),
        Type::Bytes => Some(LoweredType::Bytes),
        Type::Digest => Some(LoweredType::Digest),
        Type::Regex => Some(LoweredType::Regex),
        Type::Status => Some(LoweredType::Status),
        Type::Path => Some(LoweredType::Path),
        Type::Command => Some(LoweredType::Command),
        Type::ProcessHandle => Some(LoweredType::ProcessHandle),
        Type::NetJob => Some(LoweredType::NetJob),
        Type::FsRoot => Some(LoweredType::FsRoot),
        Type::FsLock => Some(LoweredType::FsLock),
        Type::Pure => Some(LoweredType::Pure),
        Type::Proc => Some(LoweredType::Proc),
        Type::Callable(callable) if callable.pure => Some(LoweredType::Pure),
        Type::Callable(_) => Some(LoweredType::Proc),
        Type::Error | Type::ErrorFamily(_) | Type::ErrorVariant { .. } | Type::ProcessError => {
            Some(LoweredType::Error)
        }
        Type::ErasedRecord | Type::Record(_) => Some(LoweredType::Record),
        Type::Module(_) | Type::DynamicModule => Some(LoweredType::Module),
        Type::List(_) => Some(LoweredType::List),
        Type::Stream(_) => Some(LoweredType::Stream),
        Type::Set(_) => Some(LoweredType::Set),
        Type::Map(_, _) => Some(LoweredType::Map),
        Type::Tag(_) => Some(LoweredType::Tag),
        Type::Result(_, _) => Some(LoweredType::Result),
        Type::Any | Type::Unknown | Type::Invalid => Some(LoweredType::Any),
        // A union value is stored as whichever member it is, so its slot
        // has no single storage kind.
        Type::Union(_) => Some(LoweredType::Any),
        // A validated value is stored as a value of its base.
        Type::Validated(validated) => lowered_checked_type(validated.base()),
        _ => None,
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

fn lowered_builtin_type_name(name: &str) -> Option<LoweredType> {
    match BuiltinTypeName::parse(name)? {
        BuiltinTypeName::Any | BuiltinTypeName::Unknown => Some(LoweredType::Any),
        BuiltinTypeName::Unit => Some(LoweredType::Unit),
        BuiltinTypeName::Int | BuiltinTypeName::UInt => Some(LoweredType::Int),
        BuiltinTypeName::Float => Some(LoweredType::Float),
        BuiltinTypeName::Duration => Some(LoweredType::Duration),
        BuiltinTypeName::Bool => Some(LoweredType::Bool),
        BuiltinTypeName::Str => Some(LoweredType::Str),
        BuiltinTypeName::Bytes => Some(LoweredType::Bytes),
        BuiltinTypeName::Digest => Some(LoweredType::Digest),
        BuiltinTypeName::Regex => Some(LoweredType::Regex),
        BuiltinTypeName::Status => Some(LoweredType::Status),
        BuiltinTypeName::Path | BuiltinTypeName::RelPath => Some(LoweredType::Path),
        BuiltinTypeName::Command => Some(LoweredType::Command),
        BuiltinTypeName::ProcessHandle => Some(LoweredType::ProcessHandle),
        BuiltinTypeName::NetJob => Some(LoweredType::NetJob),
        BuiltinTypeName::FsRoot => Some(LoweredType::FsRoot),
        BuiltinTypeName::FsLock => Some(LoweredType::FsLock),
        BuiltinTypeName::Pure => Some(LoweredType::Pure),
        BuiltinTypeName::Proc => Some(LoweredType::Proc),
        BuiltinTypeName::Error | BuiltinTypeName::ProcessError => Some(LoweredType::Error),
        BuiltinTypeName::Record => Some(LoweredType::Record),
        BuiltinTypeName::Module => Some(LoweredType::Module),
        BuiltinTypeName::Result => Some(LoweredType::Result),
        BuiltinTypeName::Null | BuiltinTypeName::Map | BuiltinTypeName::EnvPathList => None,
    }
}

pub(super) fn type_for_lowered_type(kind: LoweredType) -> Option<Type> {
    match kind {
        LoweredType::Unit => Some(Type::Unit),
        LoweredType::Int => Some(Type::Int),
        LoweredType::Float => Some(Type::Float),
        LoweredType::Duration => Some(Type::Duration),
        LoweredType::Bool => Some(Type::Bool),
        LoweredType::Str => Some(Type::Str),
        LoweredType::Bytes => Some(Type::Bytes),
        LoweredType::Digest => Some(Type::Digest),
        LoweredType::Regex => Some(Type::Regex),
        LoweredType::Status => Some(Type::Status),
        LoweredType::Path => Some(Type::Path),
        LoweredType::Command => Some(Type::Command),
        LoweredType::ProcessHandle => Some(Type::ProcessHandle),
        LoweredType::NetJob => Some(Type::NetJob),
        LoweredType::FsRoot => Some(Type::FsRoot),
        LoweredType::FsLock => Some(Type::FsLock),
        LoweredType::Pure => Some(Type::Pure),
        LoweredType::Proc => Some(Type::Proc),
        LoweredType::Error => Some(Type::Error),
        LoweredType::Record => Some(Type::ErasedRecord),
        LoweredType::Module => Some(Type::DynamicModule),
        LoweredType::List => Some(Type::List(Box::new(Type::Any))),
        LoweredType::Stream => Some(Type::Stream(Box::new(Type::Any))),
        LoweredType::Map => Some(Type::Map(Box::new(Type::Str), Box::new(Type::Any))),
        // The element type is not part of the kind.
        LoweredType::Set => None,
        LoweredType::Tag => None,
        LoweredType::Result => Some(Type::Result(Box::new(Type::Any), Box::new(Type::Error))),
        LoweredType::Any => Some(Type::Any),
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

    pub(super) fn lower_binding_checked_type(&self, ty: Option<TypeExprId>, value: ExprId) -> Option<Type> {
        ty.map(|ty| {
            compact_runtime_type_in_namespace(
                &self.program.arena,
                ty,
                self.declarations,
                self.current_namespace,
            )
        })
        .or_else(|| {
            self.bodies
                .expr_types
                .get(&value)
                .filter(|ty| !matches!(ty, Type::Invalid))
                .cloned()
        })
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
        let Some(ty) = ty else {
            return Some(match checked_ty {
                Some(ty) => self.checked_unsigned_value(lowered, ty, span),
                None => lowered,
            });
        };
        let kind = self
            .bodies
            .expr_types
            .get(&value)
            .map(|ty| ty.optional_inner().unwrap_or(ty))
            .and_then(lowered_checked_type)
            .unwrap_or(LoweredType::Any);
        if lowered_type_needs_static_check(kind) || matches!(checked_ty, Some(Type::UInt)) {
            let check = LoweredTypeCheck {
                schema: None,
                ty: checked_ty.cloned().unwrap_or_else(|| {
                    compact_runtime_type_in_namespace(
                        &self.program.arena,
                        ty,
                        self.declarations,
                        self.current_namespace,
                    )
                }),
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
        if let ArenaExprKind::Ident(binding) = self.program.arena.expr(base).kind
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
                ArenaExprKind::Ident(binding) if slots.resolve(binding).is_some()
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
        let tag = self.program.arena.type_expr_tags[ty.index()];
        let data = self.program.arena.type_expr_data[ty.index()];
        if tag == ArenaTypeExprTag::Result {
            return Some(LoweredReturnKind::Result(lowered_arena_type(
                &self.program.arena,
                TypeExprId::from_index(data.lhs as usize),
                self.declarations,
            )?));
        }
        if tag == ArenaTypeExprTag::Optional
            && self.program.arena.type_expr_tags[data.lhs as usize] == ArenaTypeExprTag::Result
        {
            return Some(LoweredReturnKind::OptionalResult);
        }
        Some(LoweredReturnKind::Plain(lowered_arena_type(
            &self.program.arena,
            ty,
            self.declarations,
        )?))
    }

    pub(super) fn assign_target_checked_type(
        &self,
        id: crate::syntax::arena::AssignTargetId,
        slots: &SlotScope,
    ) -> Option<Type> {
        match self.program.arena.assign_target(id).kind {
            ArenaAssignTargetKind::Name(name) => slots.binding_type(name).cloned(),
            ArenaAssignTargetKind::Field { base, name } => {
                match self.assign_target_checked_type(base, slots)? {
                    Type::Record(fields) => fields.get(&name).cloned(),
                    _ => None,
                }
            }
            ArenaAssignTargetKind::Index { base, .. } => {
                match self
                    .assign_target_checked_type(base, slots)?
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
